//! Wii Remotes connected by the launcher itself, in player slots J1–J4.
//! One task owns all the state; Tauri commands and the per-remote readers talk
//! to it through a channel, and every change goes out as a `wiimotes://state` event.

mod bridge;
mod protocol;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod transport;
#[cfg(windows)]
#[path = "windows.rs"]
mod transport;
#[cfg(not(any(target_os = "linux", windows)))]
#[path = "unsupported.rs"]
mod transport;

use crate::i18n::tr;
use bridge::Proxy;
use protocol::Input;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::async_runtime::{spawn, JoinHandle};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{mpsc, oneshot};
use transport::{Bluetooth, Discovery, Link};

const STATE_EVENT: &str = "wiimotes://state";
/// J1 drives the launcher: where it points, and its button presses/releases.
const POINTER_EVENT: &str = "wiimotes://pointer";
const BUTTON_EVENT: &str = "wiimotes://button";
const POINTER_EVERY: Duration = Duration::from_millis(15);
const SLOTS: usize = 4;
const TICK: Duration = Duration::from_secs(1);
const BATTERY_EVERY: Duration = Duration::from_secs(60);
const RETRY_AFTER: Duration = Duration::from_secs(5);

/// Why Bluetooth can't be used (what transports report from `open`).
pub enum Availability {
    NoAdapter,
    #[cfg_attr(any(target_os = "linux", windows), allow(dead_code))]
    Unsupported,
    Error(String),
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// "ready", "off", "none", "unsupported" or "error".
    bluetooth: &'static str,
    message: Option<String>,
    scanning: bool,
    /// The game is running and the remotes are lent to it.
    paused: bool,
    /// Remotes being connected right now (they'll land in free slots).
    connecting: usize,
    slots: [Option<RemoteView>; SLOTS],
    /// Remotes the system's own Bluetooth/HID driver holds, unusable until released.
    system: Vec<SystemRemote>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RemoteView {
    addr: String,
    model: &'static str,
    battery: Option<u8>,
    extension: Option<String>,
    motion_plus: bool,
    buttons: Vec<&'static str>,
}

/// What J1's IR camera sees (camera pixels) and where that puts its aim; and,
/// with a MotionPlus, the degrees (yaw, pitch) it turned since the last event.
#[derive(Serialize, Clone)]
struct PointerEvent {
    dots: Vec<(f32, f32)>,
    aim: Option<(f32, f32)>,
    turn: Option<(f32, f32)>,
}

#[derive(Serialize, Clone)]
struct ButtonEvent {
    button: &'static str,
    down: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SystemRemote {
    addr: String,
    model: &'static str,
}

enum Msg {
    Get(oneshot::Sender<Snapshot>),
    Scan(bool),
    Disconnect(usize),
    Identify(usize),
    Move(usize, usize),
    Release(String, oneshot::Sender<Result<(), String>>),
    /// The game starts: answers with the socket for Dolphin, if remotes can go through it.
    Lend(oneshot::Sender<Option<PathBuf>>),
    TakeBack,
    /// Dolphin asks for the remote in a slot.
    #[cfg(unix)]
    Open { slot: usize, stream: tokio::net::UnixStream },
    Battery { id: u64, raw: u8 },
    /// Dolphin closed a remote's socket; it can ask for it again while the game runs.
    #[cfg(unix)]
    LetGo { id: u64 },
    Connected { addr: String, name: String, link: Result<Link, String> },
    Input { id: u64, report: Vec<u8> },
    Lost { id: u64 },
    Tick,
}

/// Handle kept in Tauri's state; cheap to clone.
#[derive(Clone)]
pub struct Wiimotes(mpsc::UnboundedSender<Msg>);

impl Wiimotes {
    /// While the game runs its reports go to Dolphin, through the socket this returns
    /// (None: Dolphin has to look for remotes itself).
    pub async fn lend(&self) -> Option<PathBuf> {
        let (tx, rx) = oneshot::channel();
        self.0.send(Msg::Lend(tx)).ok()?;
        rx.await.ok().flatten()
    }

    pub fn take_back(&self) {
        let _ = self.0.send(Msg::TakeBack);
    }

    /// Whether someone can play: a remote is connected. Where the launcher can't
    /// connect any yet (macOS), only Dolphin can tell, so this says yes.
    pub async fn has_player(&self) -> bool {
        let (tx, rx) = oneshot::channel();
        if self.0.send(Msg::Get(tx)).is_err() {
            return false;
        }
        rx.await.is_ok_and(|s| s.bluetooth == "unsupported" || s.slots.iter().any(Option::is_some))
    }
}

pub fn start(app: AppHandle) -> Wiimotes {
    let (tx, rx) = mpsc::unbounded_channel();
    let ticker = tx.clone();
    spawn(async move {
        loop {
            tokio::time::sleep(TICK).await;
            if ticker.send(Msg::Tick).is_err() {
                break;
            }
        }
    });
    spawn(Manager::new(app, tx.clone()).run(rx));
    Wiimotes(tx)
}

#[derive(PartialEq)]
enum MotionPlusState {
    Probing,
    Absent,
    Starting,
    Active,
}

/// What a remote lent to the game goes through.
struct Lent {
    proxy: Proxy,
    to_game: mpsc::UnboundedSender<Vec<u8>>,
}

struct Remote {
    id: u64,
    addr: String,
    name: String,
    link: Arc<Link>,
    reader: JoinHandle<()>,
    /// Shared with the reader, which passes reports on without waiting for the manager.
    lent: Arc<Mutex<Option<Lent>>>,
    serving: Option<JoinHandle<()>>,
    battery: Option<u8>,
    extension: Option<&'static str>,
    buttons: u16,
    last_status: Instant,
    aim: protocol::Aim,
    dots_seen: usize,
    last_pointer: Instant,
    motion_plus: MotionPlusState,
    gyro: protocol::Gyro,
    turn: (f32, f32),
    last_sample: Instant,
}

impl Remote {
    /// Records new button state; `drives_ui`: sends each press and release to the launcher UI.
    fn set_buttons(&mut self, app: &AppHandle, drives_ui: bool, buttons: u16) {
        if drives_ui {
            let changed = |bits: u16| protocol::button_names(bits);
            for button in changed(buttons & !self.buttons) {
                let _ = app.emit(BUTTON_EVENT, ButtonEvent { button, down: true });
            }
            for button in changed(self.buttons & !buttons) {
                let _ = app.emit(BUTTON_EVENT, ButtonEvent { button, down: false });
            }
        }
        self.buttons = buttons;
    }
}

impl Drop for Remote {
    // The reader holds the other reference to the link: once it is gone the
    // sockets close and the remote switches itself off.
    fn drop(&mut self) {
        self.reader.abort();
        if let Some(serving) = &self.serving {
            serving.abort();
        }
    }
}

struct Manager {
    app: AppHandle,
    tx: mpsc::UnboundedSender<Msg>,
    bt: Result<Arc<Bluetooth>, Availability>,
    powered: bool,
    scanning: bool,
    paused: bool,
    /// In game: looking for one more player, at the user's request.
    joining: bool,
    /// In game without the bridge (Windows): Dolphin opens the remotes itself, the
    /// launcher keeps their links but leaves them alone.
    shared: bool,
    /// Remotes in the game, and where one that drops out belongs.
    players: usize,
    places: HashMap<String, usize>,
    listener: Option<(PathBuf, JoinHandle<()>)>,
    discovery: Option<Discovery>,
    slots: [Option<Remote>; SLOTS],
    connecting: HashSet<String>,
    failed: HashMap<String, Instant>,
    system: Vec<SystemRemote>,
    next_id: u64,
    ticks: u64,
    published: String,
}

impl Manager {
    fn new(app: AppHandle, tx: mpsc::UnboundedSender<Msg>) -> Self {
        Manager {
            app,
            tx,
            bt: Err(Availability::Error(tr!("Démarrage…"))),
            powered: false,
            scanning: false,
            paused: false,
            joining: false,
            shared: false,
            players: 0,
            places: HashMap::new(),
            listener: None,
            discovery: None,
            slots: Default::default(),
            connecting: HashSet::new(),
            failed: HashMap::new(),
            system: Vec::new(),
            next_id: 0,
            ticks: 0,
            published: String::new(),
        }
    }

    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Msg>) {
        self.bt = Bluetooth::open().await.map(Arc::new);
        self.refresh().await;
        self.publish();
        while let Some(msg) = rx.recv().await {
            self.handle(msg).await;
            self.publish();
        }
    }

    async fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Get(reply) => {
                let _ = reply.send(self.snapshot());
            }
            Msg::Scan(on) => {
                if self.paused {
                    self.joining = on;
                } else {
                    self.scanning = on;
                }
                self.discovery = None;
                self.refresh().await;
            }
            Msg::Lend(reply) => {
                self.paused = true;
                self.joining = false;
                self.players = self.slots.iter().flatten().count();
                self.places.clear();
                self.discovery = None;
                let (app, slots) = (&self.app, &mut self.slots);
                // The launcher stops following J1: nothing stays held or shown.
                for (slot, r) in slots.iter_mut().enumerate() {
                    if let Some(r) = r {
                        r.set_buttons(app, slot == 0, 0);
                    }
                }
                let _ = app.emit(POINTER_EVENT, PointerEvent { dots: Vec::new(), aim: None, turn: None });
                let supported = !matches!(self.bt, Err(Availability::Unsupported));
                self.listener = if supported { listen(self.tx.clone()) } else { None };
                self.shared = self.listener.is_none();
                if self.shared {
                    self.release_motion_plus().await;
                }
                let _ = reply.send(self.listener.as_ref().map(|(path, _)| path.clone()));
            }
            Msg::TakeBack => {
                self.paused = false;
                self.joining = false;
                self.places.clear();
                // Dolphin set the remotes up its own way.
                if std::mem::take(&mut self.shared) {
                    for (slot, r) in self.slots.iter_mut().enumerate() {
                        let Some(r) = r else { continue };
                        (r.motion_plus, r.extension) = (MotionPlusState::Probing, None);
                        (r.aim, r.gyro, r.turn) = Default::default();
                        spawn(set_up(r.link.clone(), slot, true));
                    }
                }
                if let Some((path, task)) = self.listener.take() {
                    task.abort();
                    let _ = std::fs::remove_file(path);
                }
                for slot in 0..SLOTS {
                    self.take_back(slot);
                }
                self.refresh().await;
            }
            #[cfg(unix)]
            Msg::Open { slot, stream } => self.open(slot, stream),
            #[cfg(unix)]
            Msg::LetGo { id } => {
                if let Some(slot) = self.slots.iter().position(|r| r.as_ref().is_some_and(|r| r.id == id)) {
                    self.take_back(slot);
                }
            }
            Msg::Battery { id, raw } => {
                if let Some(r) = self.slots.iter_mut().flatten().find(|r| r.id == id) {
                    r.battery = Some(protocol::battery_percent(raw));
                }
            }
            Msg::Tick => {
                self.ticks += 1;
                self.refresh().await;
            }
            Msg::Disconnect(slot) => {
                if slot < SLOTS && self.slots[slot].take().is_some() && self.paused {
                    self.players = self.players.saturating_sub(1);
                }
            }
            Msg::Identify(slot) => {
                if let Some(r) = self.slots.get(slot).and_then(Option::as_ref) {
                    let link = r.link.clone();
                    spawn(async move {
                        let _ = link.send(&protocol::rumble(true)).await;
                        tokio::time::sleep(Duration::from_millis(400)).await;
                        let _ = link.send(&protocol::rumble(false)).await;
                    });
                }
            }
            Msg::Move(from, to) => {
                // Dolphin numbers the remotes it opened itself.
                if from < SLOTS && to < SLOTS && from != to && !self.shared {
                    self.slots.swap(from, to);
                    for slot in [from, to] {
                        // In game, a socket is one player: Dolphin asks again for both.
                        self.take_back(slot);
                        if let Some(r) = &self.slots[slot] {
                            let _ = r.link.send(&protocol::leds(slot)).await;
                        }
                    }
                }
            }
            Msg::Release(addr, reply) => {
                let result = match &self.bt {
                    Ok(bt) => bt.release(&addr).await,
                    Err(_) => Err(tr!("Bluetooth indisponible")),
                };
                let _ = reply.send(result);
                self.refresh().await;
            }
            Msg::Connected { addr, name, link } => {
                self.connecting.remove(&addr);
                match link {
                    Ok(link) => self.add(addr, name, link).await,
                    Err(_) => {
                        self.failed.insert(addr, Instant::now());
                    }
                }
            }
            // Answering the reports the game asked for would get in its way.
            Msg::Input { id, report } if !self.shared => self.input(id, &report).await,
            Msg::Input { .. } => {}
            Msg::Lost { id } => {
                for (i, slot) in self.slots.iter_mut().enumerate() {
                    if slot.as_ref().is_some_and(|r| r.id == id) {
                        if let (true, Some(r)) = (self.paused, slot.take()) {
                            self.places.insert(r.addr.clone(), i);
                        }
                        *slot = None;
                    }
                }
            }
        }
    }

    /// Polls BlueZ: power, remotes held by the system, and (while scanning)
    /// remotes in 1+2 mode to connect.
    async fn refresh(&mut self) {
        if let Err(Availability::NoAdapter | Availability::Error(_)) = self.bt {
            if self.ticks % 5 == 0 {
                self.bt = Bluetooth::open().await.map(Arc::new);
            }
        }
        let Ok(bt) = self.bt.as_ref().map(Arc::clone) else { return };

        self.powered = bt.powered().await;
        // Discovery slows Bluetooth down: in game, only to find a remote that dropped out or a new player.
        let filled = self.slots.iter().flatten().count();
        let looking = if self.paused { !self.shared && (self.joining || filled < self.players) } else { self.scanning };
        if !self.powered || !looking {
            self.discovery = None;
        } else if self.discovery.is_none() {
            self.discovery = bt.discover().await.ok();
        }

        let ours: HashSet<String> =
            self.slots.iter().flatten().map(|r| r.addr.clone()).chain(self.connecting.iter().cloned()).collect();
        let seen = bt.remotes().await;
        self.system = seen
            .iter()
            .filter(|s| s.connected && !ours.contains(&s.addr))
            .map(|s| SystemRemote { addr: s.addr.clone(), model: protocol::model(&s.name) })
            .collect();

        if self.discovery.is_some() {
            let mut free = self.slots.iter().filter(|s| s.is_none()).count().saturating_sub(self.connecting.len());
            for s in seen {
                if free == 0 {
                    break;
                }
                let cooling = self.failed.get(&s.addr).is_some_and(|t| t.elapsed() < RETRY_AFTER);
                if !s.in_range || s.connected || ours.contains(&s.addr) || cooling {
                    continue;
                }
                free -= 1;
                self.connecting.insert(s.addr.clone());
                let (bt, tx) = (bt.clone(), self.tx.clone());
                spawn(async move {
                    let link = bt.connect(&s.addr).await;
                    let _ = tx.send(Msg::Connected { addr: s.addr, name: s.name, link });
                });
            }
        }

        // A remote lent to the game answers the game's own status requests.
        for r in self.slots.iter_mut().flatten().filter(|r| r.serving.is_none() && !self.shared) {
            if r.last_status.elapsed() >= BATTERY_EVERY {
                r.last_status = Instant::now();
                let _ = r.link.send(&protocol::status_request()).await;
            }
        }
    }

    async fn add(&mut self, addr: String, name: String, link: Link) {
        let place = self.places.remove(&addr).filter(|&i| self.slots[i].is_none());
        let returning = place.is_some();
        let Some(slot) = place.or_else(|| self.slots.iter().position(Option::is_none)) else { return };
        let id = self.next_id;
        self.next_id += 1;
        let link = Arc::new(link);

        let lent: Arc<Mutex<Option<Lent>>> = Arc::default();

        let (reader_link, reader_lent, tx) = (link.clone(), lent.clone(), self.tx.clone());
        let reader = spawn(async move {
            let mut buf = [0u8; 32];
            loop {
                let n = match reader_link.recv(&mut buf).await {
                    Ok(n) => n,
                    Err(_) => {
                        let _ = tx.send(Msg::Lost { id });
                        break;
                    }
                };
                let report = &buf[..n];
                let answer = {
                    let mut lent = reader_lent.lock().unwrap();
                    lent.as_mut().map(|lent| {
                        let forward = lent.proxy.from_remote(report, Instant::now());
                        for r in forward.to_game {
                            let _ = lent.to_game.send(r);
                        }
                        forward.to_remote
                    })
                };
                let message = match answer {
                    None => Msg::Input { id, report: report.to_vec() },
                    Some(to_remote) => {
                        for r in to_remote {
                            let _ = reader_link.send(&r).await;
                        }
                        let Input::Status { battery, .. } = protocol::parse(report) else { continue };
                        Msg::Battery { id, raw: battery }
                    }
                };
                if tx.send(message).is_err() {
                    break;
                }
            }
        });

        if self.paused && !returning {
            self.joining = false;
            self.players += 1;
        }
        spawn(set_up(link.clone(), slot, true));
        self.slots[slot] = Some(Remote {
            id,
            addr,
            name,
            link,
            reader,
            lent,
            serving: None,
            battery: None,
            extension: None,
            buttons: 0,
            last_status: Instant::now(),
            aim: Default::default(),
            dots_seen: 0,
            last_pointer: Instant::now(),
            motion_plus: MotionPlusState::Probing,
            gyro: Default::default(),
            turn: (0.0, 0.0),
            last_sample: Instant::now(),
        });
    }

    async fn input(&mut self, id: u64, report: &[u8]) {
        let Some(slot) = self.slots.iter().position(|r| r.as_ref().is_some_and(|r| r.id == id)) else { return };
        let drives_ui = slot == 0 && !self.paused;
        let (app, Some(r)) = (&self.app, &mut self.slots[slot]) else { return };
        match protocol::parse(report) {
            Input::Status { buttons, extension, battery } => {
                r.set_buttons(app, drives_ui, buttons);
                r.battery = Some(protocol::battery_percent(battery));
                // A status report stops data reporting until the mode is set again.
                let _ = r.link.send(&protocol::report_mode()).await;
                let motion_plus_on = matches!(r.motion_plus, MotionPlusState::Starting | MotionPlusState::Active);
                if !extension {
                    r.extension = None;
                    if motion_plus_on {
                        r.motion_plus = MotionPlusState::Absent;
                    }
                } else if motion_plus_on {
                    // That's the MotionPlus itself; what is plugged into it shows in its data.
                } else if r.extension.is_none() {
                    r.extension = Some("Extension");
                    let link = r.link.clone();
                    spawn(async move {
                        for (addr, value) in protocol::EXTENSION_INIT {
                            let _ = link.send(&protocol::write_register(addr, &[value])).await;
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                        let _ = link.send(&protocol::read_register(protocol::EXTENSION_ID, 6)).await;
                    });
                }
            }
            // Both ids answer at 0x00FA: the MotionPlus probe is the first read sent.
            Input::Read { buttons, addr, data } if addr == 0x00FA && r.motion_plus == MotionPlusState::Probing => {
                r.set_buttons(app, drives_ui, buttons);
                if data.as_deref().is_some_and(protocol::is_motion_plus) {
                    r.motion_plus = MotionPlusState::Starting;
                    let (addr, value) = protocol::MOTION_PLUS_START;
                    let _ = r.link.send(&protocol::write_register(addr, &[value])).await;
                } else {
                    r.motion_plus = MotionPlusState::Absent;
                }
            }
            Input::Read { buttons, addr, data } => {
                r.set_buttons(app, drives_ui, buttons);
                if let (0x00FA, Some(data), Some(_)) = (addr, data, r.extension) {
                    r.extension = Some(protocol::extension_name(&data));
                }
            }
            Input::Data { buttons, dots, ext } => {
                r.set_buttons(app, drives_ui, buttons);
                let aim = r.aim.update(&dots);
                let now = Instant::now();
                let dt = now.duration_since(r.last_sample).as_secs_f32().min(0.05);
                r.last_sample = now;
                let sample = matches!(r.motion_plus, MotionPlusState::Starting | MotionPlusState::Active)
                    .then(|| protocol::motion_plus(&ext))
                    .flatten();
                if let Some(m) = sample {
                    r.motion_plus = MotionPlusState::Active;
                    r.extension = m.extension.then_some("Extension");
                    if let Some((yaw, pitch)) = r.gyro.update(&m, dt) {
                        r.turn = (r.turn.0 + yaw, r.turn.1 + pitch);
                    }
                }
                if drives_ui && (dots.len() != r.dots_seen || r.last_pointer.elapsed() >= POINTER_EVERY) {
                    r.dots_seen = dots.len();
                    r.last_pointer = now;
                    let turn = (r.motion_plus == MotionPlusState::Active && r.gyro.ready()).then(|| std::mem::take(&mut r.turn));
                    let _ = app.emit(POINTER_EVENT, PointerEvent { dots, aim, turn });
                }
            }
            Input::Buttons(buttons) => r.set_buttons(app, drives_ui, buttons),
            Input::Other => {}
        }
    }

    /// Before Dolphin opens the remotes itself (no bridge: Windows): the MotionPlus the
    /// launcher turned on goes off, as on a Wii, where a game that doesn't use it never
    /// sees it. Left on, the game takes it for an extension it doesn't know and gets no
    /// pointer. TakeBack looks for it again afterwards.
    async fn release_motion_plus(&mut self) {
        let (addr, value) = protocol::EXTENSION_INIT[0];
        for r in self.slots.iter_mut().flatten() {
            if matches!(r.motion_plus, MotionPlusState::Starting | MotionPlusState::Active) {
                let _ = r.link.send(&protocol::write_register(addr, &[value])).await;
                r.motion_plus = MotionPlusState::Absent;
            }
        }
    }

    /// Puts a remote lent to the game back to how the launcher uses it.
    fn take_back(&mut self, slot: usize) {
        let Some(r) = &mut self.slots[slot] else { return };
        if let Some(serving) = r.serving.take() {
            serving.abort();
        }
        let lent = r.lent.lock().unwrap().take();
        let Some(lent) = lent else { return };
        let probe = lent.proxy.motion_plus_off;
        if probe {
            r.motion_plus = MotionPlusState::Probing;
            r.extension = None;
        }
        (r.aim, r.gyro, r.turn) = Default::default();
        spawn(set_up(r.link.clone(), slot, probe));
    }

    #[cfg(unix)]
    fn open(&mut self, slot: usize, mut stream: tokio::net::UnixStream) {
        use tokio::io::AsyncWriteExt;
        let remote = self.slots.get_mut(slot).and_then(Option::as_mut).filter(|r| {
            // The MotionPlus has to be sorted out first: Dolphin will ask again.
            let settled = matches!(r.motion_plus, MotionPlusState::Absent | MotionPlusState::Active);
            settled && r.serving.is_none()
        });
        let Some(r) = remote.filter(|_| self.paused) else {
            spawn(async move {
                let _ = stream.write_all(&[0]).await;
            });
            return;
        };
        let proxy = Proxy::new(r.motion_plus == MotionPlusState::Active, std::mem::take(&mut r.gyro));
        let first = proxy.start();
        let (to_game, from_remote) = mpsc::unbounded_channel();
        *r.lent.lock().unwrap() = Some(Lent { proxy, to_game });
        let (link, lent) = (r.link.clone(), r.lent.clone());
        let (id, tx) = (r.id, self.tx.clone());
        r.serving = Some(spawn(async move {
            if let Some(first) = first {
                let _ = link.send(&first).await;
            }
            serve(stream, link, lent, from_remote).await;
            let _ = tx.send(Msg::LetGo { id });
        }));
    }

    fn snapshot(&self) -> Snapshot {
        let (bluetooth, message) = match &self.bt {
            Ok(_) if self.powered => ("ready", None),
            Ok(_) => ("off", None),
            Err(Availability::NoAdapter) => ("none", None),
            Err(Availability::Unsupported) => ("unsupported", None),
            Err(Availability::Error(m)) => ("error", Some(m.clone())),
        };
        Snapshot {
            bluetooth,
            message,
            scanning: if self.paused { self.joining } else { self.scanning },
            paused: self.paused,
            connecting: self.connecting.len(),
            slots: std::array::from_fn(|i| {
                self.slots[i].as_ref().map(|r| RemoteView {
                    addr: r.addr.clone(),
                    model: protocol::model(&r.name),
                    battery: r.battery,
                    extension: r.extension.map(protocol::extension_label),
                    motion_plus: r.motion_plus == MotionPlusState::Active,
                    buttons: protocol::button_names(r.buttons),
                })
            }),
            system: self.system.clone(),
        }
    }

    fn publish(&mut self) {
        let snapshot = self.snapshot();
        let json = serde_json::to_string(&snapshot).unwrap_or_default();
        if json != self.published {
            self.published = json;
            let _ = self.app.emit(STATE_EVENT, snapshot);
        }
    }
}

/// Reports the launcher sends a remote to set it up for its own use.
async fn set_up(link: Arc<Link>, slot: usize, probe_motion_plus: bool) {
    let (addr, value) = protocol::MOTION_PLUS_INIT;
    let probe = [protocol::write_register(addr, &[value]), protocol::read_register(protocol::MOTION_PLUS_ID, 6)];
    let reports = [protocol::rumble(false), protocol::leds(slot)]
        .into_iter()
        .chain(protocol::ir_setup())
        .chain([protocol::report_mode()])
        .chain(probe.into_iter().filter(|_| probe_motion_plus))
        .chain([protocol::status_request()]);
    for report in reports {
        let _ = link.send(&report).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(not(unix))]
fn listen(_: mpsc::UnboundedSender<Msg>) -> Option<(PathBuf, JoinHandle<()>)> {
    None
}

/// Opens the socket Dolphin asks for remotes through.
#[cfg(unix)]
fn listen(tx: mpsc::UnboundedSender<Msg>) -> Option<(PathBuf, JoinHandle<()>)> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::AsyncReadExt;

    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let path = dir.join(format!("wp-launcher-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::UnixListener::bind(&path).ok()?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok()?;
    let task = spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let tx = tx.clone();
            spawn(async move {
                let asked = tokio::time::timeout(Duration::from_secs(2), stream.read_u8()).await;
                if let Ok(Ok(slot)) = asked {
                    let _ = tx.send(Msg::Open { slot: slot as usize, stream });
                }
            });
        }
    });
    Some((path, task))
}

/// Carries one remote's reports between Dolphin and the proxy until Dolphin lets go.
/// Each is preceded by its length.
#[cfg(unix)]
async fn serve(
    stream: tokio::net::UnixStream,
    link: Arc<Link>,
    lent: Arc<Mutex<Option<Lent>>>,
    mut from_remote: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (mut read, mut write) = stream.into_split();
    if write.write_all(&[1]).await.is_err() {
        return;
    }
    let writer = spawn(async move {
        while let Some(report) = from_remote.recv().await {
            let mut frame = vec![report.len() as u8];
            frame.extend_from_slice(&report);
            if write.write_all(&frame).await.is_err() {
                break;
            }
        }
    });
    let mut buf = [0u8; 255];
    while let Ok(size) = read.read_u8().await {
        let report = &mut buf[..size as usize];
        if read.read_exact(report).await.is_err() {
            break;
        }
        let to_remote = {
            let mut lent = lent.lock().unwrap();
            let Some(lent) = lent.as_mut() else { break };
            let forward = lent.proxy.from_game(report);
            for r in forward.to_game {
                let _ = lent.to_game.send(r);
            }
            forward.to_remote
        };
        for r in to_remote {
            if link.send(&r).await.is_err() {
                break;
            }
        }
    }
    writer.abort();
}

fn send(w: &Wiimotes, msg: Msg) -> Result<(), String> {
    w.0.send(msg).map_err(|_| tr!("Gestionnaire de manettes arrêté"))
}

#[tauri::command]
pub async fn wiimote_state(w: State<'_, Wiimotes>) -> Result<Snapshot, String> {
    let (tx, rx) = oneshot::channel();
    send(&w, Msg::Get(tx))?;
    rx.await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn wiimote_scan(w: State<'_, Wiimotes>, on: bool) -> Result<(), String> {
    send(&w, Msg::Scan(on))
}

#[tauri::command]
pub fn wiimote_disconnect(w: State<'_, Wiimotes>, slot: usize) -> Result<(), String> {
    send(&w, Msg::Disconnect(slot))
}

#[tauri::command]
pub fn wiimote_identify(w: State<'_, Wiimotes>, slot: usize) -> Result<(), String> {
    send(&w, Msg::Identify(slot))
}

#[tauri::command]
pub fn wiimote_move(w: State<'_, Wiimotes>, from: usize, to: usize) -> Result<(), String> {
    send(&w, Msg::Move(from, to))
}

/// Disconnects a remote from the system's own Bluetooth so the launcher can take it.
#[tauri::command]
pub async fn wiimote_release(w: State<'_, Wiimotes>, addr: String) -> Result<(), String> {
    let (tx, rx) = oneshot::channel();
    send(&w, Msg::Release(addr, tx))?;
    rx.await.map_err(|e| e.to_string())?
}
