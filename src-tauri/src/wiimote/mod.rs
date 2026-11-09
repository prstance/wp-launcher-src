//! Wii Remotes connected by the launcher itself, in player slots J1–J4.
//! One task owns all the state; Tauri commands and the per-remote readers talk
//! to it through a channel, and every change goes out as a `wiimotes://state` event.

mod bridge;
mod pointer;
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
use pointer::Pointer;
use protocol::Input;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::async_runtime::{spawn, JoinHandle};
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncRead, AsyncWrite};
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
/// A MotionPlus told to start that sends no data by then is told again, then taken for absent.
const START_AGAIN_AFTER: Duration = Duration::from_secs(2);
const START_GIVE_UP_AFTER: Duration = Duration::from_secs(5);

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

/// What J1's IR camera sees (camera pixels), where it points on the screen (-1 to 1,
/// left to right and top to bottom, beyond: off screen) and from what: "bar", "gyro" or "tilt".
#[derive(Serialize, Clone)]
struct PointerEvent {
    dots: Vec<(f32, f32)>,
    pos: Option<(f32, f32)>,
    source: Option<&'static str>,
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
    /// The game starts: answers with the bridge for Dolphin (socket or pipe), if remotes
    /// can go through it.
    Lend(oneshot::Sender<Option<PathBuf>>),
    TakeBack,
    /// Dolphin asks for the remote in a slot.
    Open { slot: usize, stream: Box<dyn Stream> },
    Battery { id: u64, raw: u8 },
    /// Dolphin closed a remote's socket; it can ask for it again while the game runs.
    LetGo { id: u64 },
    Connected { addr: String, name: String, link: Result<Link, String> },
    Input { id: u64, report: Vec<u8> },
    Lost { id: u64 },
    /// Where the sensor bar sits: on top of the screen, or below it.
    SetBar(bool),
    Tick,
}

/// Dolphin's end of the bridge, one per slot: a Unix socket, or a named pipe on Windows.
trait Stream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Stream for T {}

/// Handle kept in Tauri's state; cheap to clone.
#[derive(Clone)]
pub struct Wiimotes(mpsc::UnboundedSender<Msg>);

impl Wiimotes {
    /// While the game runs its reports go to Dolphin, through the socket (Windows: the
    /// named pipe) this returns (None: Dolphin has to look for remotes itself).
    pub async fn lend(&self) -> Option<PathBuf> {
        let (tx, rx) = oneshot::channel();
        self.0.send(Msg::Lend(tx)).ok()?;
        rx.await.ok().flatten()
    }

    pub fn take_back(&self) {
        let _ = self.0.send(Msg::TakeBack);
    }

    pub fn set_bar(&self, top: bool) {
        let _ = self.0.send(Msg::SetBar(top));
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
    /// One writer sends the remote what the proxy decided, in the order it decided it
    /// (the reader and `serve` both queue there while holding the lock).
    to_remote: mpsc::UnboundedSender<Vec<u8>>,
}

/// A remote Dolphin opened itself (no bridge): the launcher let go of it without
/// disconnecting it, and takes it back into its slot after the game.
struct Parked {
    addr: String,
    name: String,
    pointer: Pointer,
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
    /// Reports being sent to set the remote (or its extension) up: the game only gets it
    /// once they are all out.
    setting_up: Vec<JoinHandle<()>>,
    battery: Option<u8>,
    extension: Option<&'static str>,
    buttons: u16,
    last_status: Instant,
    dots_seen: usize,
    last_pointer: Instant,
    motion_plus: MotionPlusState,
    /// When the MotionPlus was told to start, and whether it was told again.
    starting_since: Instant,
    start_resent: bool,
    pointer: Pointer,
    /// The MotionPlus's calibration, which comes in two reads of 16 bytes.
    cal_read: Vec<u8>,
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

    /// Keeps the game waiting until this task has sent its reports.
    fn sending(&mut self, task: JoinHandle<()>) {
        self.setting_up.retain(|task| !task.inner().is_finished());
        self.setting_up.push(task);
    }

    fn prepare(&mut self, slot: usize, probe_motion_plus: bool) {
        self.sending(spawn(set_up(self.link.clone(), slot, probe_motion_plus)));
    }

    fn settled(&self) -> bool {
        matches!(self.motion_plus, MotionPlusState::Absent | MotionPlusState::Active)
            && self.setting_up.iter().all(|task| task.inner().is_finished())
    }

    /// A MotionPlus told to start is on (its first data came, or its id says so): its
    /// own calibration can be read now.
    async fn motion_plus_on(&mut self) {
        self.motion_plus = MotionPlusState::Active;
        self.cal_read.clear();
        let _ = self.link.send(&protocol::read_register(protocol::MOTION_PLUS_CAL, 32)).await;
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
    /// In game without the bridge (its socket or pipe couldn't be opened): Dolphin opens
    /// the remotes itself, the launcher keeps their links but leaves them alone.
    shared: bool,
    /// Remotes in the game, and where one that drops out belongs.
    players: usize,
    places: HashMap<String, usize>,
    listener: Option<(PathBuf, JoinHandle<()>)>,
    discovery: Option<Discovery>,
    slots: [Option<Remote>; SLOTS],
    parked: [Option<Parked>; SLOTS],
    /// Pointers of parked remotes being connected again, by address.
    kept: HashMap<String, Pointer>,
    bar_top: bool,
    connecting: HashSet<String>,
    failed: HashMap<String, Instant>,
    system: Vec<SystemRemote>,
    next_id: u64,
    ticks: u64,
    published: String,
}

impl Manager {
    fn new(app: AppHandle, tx: mpsc::UnboundedSender<Msg>) -> Self {
        use tauri::Manager as _;
        let bar_top = app.state::<crate::settings::Store>().get(&app).sensor_bar_top;
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
            parked: Default::default(),
            kept: HashMap::new(),
            bar_top,
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
                let _ = app.emit(POINTER_EVENT, PointerEvent { dots: Vec::new(), pos: None, source: None });
                let supported = !matches!(self.bt, Err(Availability::Unsupported));
                self.listener = if supported { listen(self.tx.clone()) } else { None };
                self.shared = self.listener.is_none();
                if self.shared {
                    self.release_motion_plus().await;
                    self.park();
                }
                let _ = reply.send(self.listener.as_ref().map(|(path, _)| path.clone()));
            }
            Msg::TakeBack => {
                self.paused = false;
                self.joining = false;
                self.shared = false;
                self.places.clear();
                self.unpark();
                if let Some((path, task)) = self.listener.take() {
                    task.abort();
                    // A pipe goes with its last handle; a socket leaves its file behind.
                    if cfg!(unix) {
                        let _ = std::fs::remove_file(path);
                    }
                }
                for slot in 0..SLOTS {
                    self.take_back(slot);
                }
                self.refresh().await;
            }
            Msg::Open { slot, stream } => self.open(slot, stream),
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
                self.check_motion_plus().await;
                self.refresh().await;
            }
            Msg::SetBar(top) => {
                self.bar_top = top;
                for r in self.slots.iter_mut().flatten() {
                    r.pointer.bar_top = top;
                }
            }
            Msg::Disconnect(slot) => {
                let Some(r) = self.slots.get_mut(slot).and_then(Option::take) else { return };
                // Windows may still list it as connected for a moment: not to be taken again.
                self.failed.insert(r.addr.clone(), Instant::now());
                if self.paused {
                    self.players = self.players.saturating_sub(1);
                }
            }
            // Dolphin's own remote (no bridge) doesn't rumble for the launcher.
            Msg::Identify(slot) if !self.shared => {
                if let Some(r) = self.slots.get(slot).and_then(Option::as_ref) {
                    let link = r.link.clone();
                    spawn(async move {
                        let _ = link.send(&protocol::rumble(true)).await;
                        tokio::time::sleep(Duration::from_millis(400)).await;
                        let _ = link.send(&protocol::rumble(false)).await;
                    });
                }
            }
            Msg::Identify(_) => {}
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
            Msg::Input { id, report } => self.input(id, &report).await,
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
        // Without the bridge on Windows, discovery has Windows connect a new player's remote,
        // which Dolphin then opens itself.
        let looking = if self.paused { self.joining || (!self.shared && filled < self.players) } else { self.scanning };
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

        // Windows connects remotes on its own (paired ones, or left on after a game): those
        // in range are already connected to it, ready to be taken.
        let adopting = cfg!(windows) && !self.paused;
        if !self.shared && (self.discovery.is_some() || adopting) {
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
                self.connect(&bt, s.addr, s.name);
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

    fn connect(&mut self, bt: &Arc<Bluetooth>, addr: String, name: String) {
        self.connecting.insert(addr.clone());
        let (bt, tx) = (bt.clone(), self.tx.clone());
        spawn(async move {
            let link = bt.connect(&addr).await;
            let _ = tx.send(Msg::Connected { addr, name, link });
        });
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
                let lent = {
                    let mut lent = reader_lent.lock().unwrap();
                    if let Some(lent) = lent.as_mut() {
                        let forward = lent.proxy.from_remote(report, Instant::now());
                        for r in forward.to_game {
                            let _ = lent.to_game.send(r);
                        }
                        for r in forward.to_remote {
                            let _ = lent.to_remote.send(r);
                        }
                    }
                    lent.is_some()
                };
                let message = if lent {
                    let Input::Status { battery, .. } = protocol::parse(report) else { continue };
                    Msg::Battery { id, raw: battery }
                } else {
                    Msg::Input { id, report: report.to_vec() }
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
        let mut pointer = self.kept.remove(&addr).unwrap_or_default();
        pointer.bar_top = self.bar_top;
        let mut remote = Remote {
            id,
            addr,
            name,
            link,
            reader,
            lent,
            serving: None,
            setting_up: Vec::new(),
            battery: None,
            extension: None,
            buttons: 0,
            last_status: Instant::now(),
            dots_seen: 0,
            last_pointer: Instant::now(),
            motion_plus: MotionPlusState::Probing,
            starting_since: Instant::now(),
            start_resent: false,
            pointer,
            cal_read: Vec::new(),
        };
        remote.prepare(slot, true);
        self.slots[slot] = Some(remote);
    }

    async fn input(&mut self, id: u64, report: &[u8]) {
        let Some(slot) = self.slots.iter().position(|r| r.as_ref().is_some_and(|r| r.id == id)) else { return };
        let drives_ui = slot == 0 && !self.paused;
        let (app, Some(r)) = (&self.app, &mut self.slots[slot]) else { return };
        // Queued before the remote was lent: the game's proxy speaks to it now.
        if r.lent.lock().unwrap().is_some() {
            return;
        }
        // Dolphin drives the remotes itself (no bridge): the launcher only
        // listens, or each status report would put them back in its own mode.
        if self.shared {
            if let Input::Status { buttons, battery, .. } = protocol::parse(report) {
                r.buttons = buttons;
                r.battery = Some(protocol::battery_percent(battery));
            }
            return;
        }
        match protocol::parse(report) {
            Input::Status { buttons, extension, battery } => {
                r.set_buttons(app, drives_ui, buttons);
                r.battery = Some(protocol::battery_percent(battery));
                // A status report stops data reporting until the mode is set again.
                let _ = r.link.send(&protocol::report_mode()).await;
                if !extension {
                    r.extension = None;
                    // While Starting, the port reads empty until the MotionPlus takes it
                    // (set_up's status request often lands then): only an active one is lost.
                    if r.motion_plus == MotionPlusState::Active {
                        r.motion_plus = MotionPlusState::Absent;
                    }
                    return;
                }
                match r.motion_plus {
                    // Its id says whether it has taken the extension's place yet.
                    MotionPlusState::Starting => {
                        let _ = r.link.send(&protocol::read_register(protocol::EXTENSION_ID, 6)).await;
                    }
                    // That's the MotionPlus itself; what is plugged into it shows in its data.
                    MotionPlusState::Active => {}
                    // set_up's status request after the probe decides: waking an extension
                    // now would turn a MotionPlus off, and its id would pass for the probe's answer.
                    MotionPlusState::Probing => {}
                    MotionPlusState::Absent if r.extension.is_some() => {}
                    MotionPlusState::Absent => {
                        r.extension = Some("Extension");
                        let link = r.link.clone();
                        r.sending(spawn(async move {
                            for (addr, value) in protocol::EXTENSION_INIT {
                                let _ = link.send(&protocol::write_register(addr, &[value])).await;
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                            let _ = link.send(&protocol::read_register(protocol::EXTENSION_ID, 6)).await;
                        }));
                    }
                }
            }
            // Both ids answer at 0x00FA: while Probing, nothing else is read there.
            Input::Read { buttons, addr, data } if addr == 0x00FA && r.motion_plus == MotionPlusState::Probing => {
                r.set_buttons(app, drives_ui, buttons);
                if data.as_deref().is_some_and(protocol::is_motion_plus) {
                    r.motion_plus = MotionPlusState::Starting;
                    (r.starting_since, r.start_resent) = (Instant::now(), false);
                    let (addr, value) = protocol::MOTION_PLUS_START;
                    let _ = r.link.send(&protocol::write_register(addr, &[value])).await;
                } else {
                    r.motion_plus = MotionPlusState::Absent;
                }
            }
            Input::Read { buttons, addr, data } if addr == 0x00FA && r.motion_plus == MotionPlusState::Starting => {
                r.set_buttons(app, drives_ui, buttons);
                if data.as_deref().is_some_and(protocol::is_active_motion_plus) {
                    r.motion_plus_on().await;
                }
            }
            Input::Read { buttons, addr, data } => {
                r.set_buttons(app, drives_ui, buttons);
                let Some(data) = data else { return };
                let active = r.motion_plus == MotionPlusState::Active;
                match addr {
                    0x00FA if r.extension.is_some() && !active => r.extension = Some(protocol::extension_name(&data)),
                    0x0020 if active => r.cal_read = data,
                    0x0030 if active && r.cal_read.len() == 16 => {
                        r.cal_read.extend(data);
                        // Implausible (or unread): Dolphin's figures stay.
                        if let Some(cal) = protocol::GyroCal::parse(&r.cal_read) {
                            r.pointer.gyro.cal = cal;
                        }
                    }
                    _ => {
                        if let Some(cal) = pointer::AccelCal::from_read(addr, &data) {
                            r.pointer.accel_cal = cal;
                        }
                    }
                }
            }
            Input::Data { buttons, dots, ext } => {
                r.set_buttons(app, drives_ui, buttons);
                let sample = matches!(r.motion_plus, MotionPlusState::Starting | MotionPlusState::Active)
                    .then(|| protocol::motion_plus(&ext))
                    .flatten();
                if let Some(m) = &sample {
                    if r.motion_plus == MotionPlusState::Starting {
                        r.motion_plus_on().await;
                    }
                    r.extension = m.extension.then_some("Extension");
                }
                let now = Instant::now();
                let seen = r.pointer.update(&dots, pointer::accel(report), sample.as_ref(), now);
                if drives_ui && (dots.len() != r.dots_seen || r.last_pointer.elapsed() >= POINTER_EVERY) {
                    r.dots_seen = dots.len();
                    r.last_pointer = now;
                    let source = seen.source.map(pointer::Source::name);
                    let _ = app.emit(POINTER_EVENT, PointerEvent { dots, pos: seen.pos, source });
                }
            }
            Input::Buttons(buttons) => r.set_buttons(app, drives_ui, buttons),
            Input::Other => {}
        }
    }

    /// A MotionPlus told to start that sends no data: told again, then taken for absent.
    async fn check_motion_plus(&mut self) {
        for r in self.slots.iter_mut().flatten().filter(|r| r.motion_plus == MotionPlusState::Starting) {
            let waited = r.starting_since.elapsed();
            if waited >= START_GIVE_UP_AFTER {
                r.motion_plus = MotionPlusState::Absent;
            } else if waited >= START_AGAIN_AFTER && !r.start_resent {
                r.start_resent = true;
                let (addr, value) = protocol::MOTION_PLUS_START;
                let _ = r.link.send(&protocol::write_register(addr, &[value])).await;
            }
        }
    }

    /// Before Dolphin opens the remotes itself (no bridge): the MotionPlus the
    /// launcher turned on goes off, as on a Wii, where a game that doesn't use it never
    /// sees it. Left on, the game takes it for an extension it doesn't know and gets no
    /// pointer. Written twice to every remote, whatever the launcher thinks it has (on
    /// one without, it only wakes the extension up): a write can get lost.
    async fn release_motion_plus(&mut self) {
        let (addr, value) = protocol::EXTENSION_INIT[0];
        for again in [false, true] {
            if again {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            for r in self.slots.iter_mut().flatten() {
                let _ = r.link.send(&protocol::write_register(addr, &[value])).await;
                r.motion_plus = MotionPlusState::Absent;
            }
        }
    }

    /// Without the bridge, the launcher lets go of its remotes without disconnecting
    /// them: nothing it does in game (a report missed, a write failed, "Déconnecter")
    /// can then cut Dolphin's remote off.
    fn park(&mut self) {
        for (slot, place) in self.slots.iter_mut().enumerate() {
            let Some(r) = place.take() else { continue };
            r.link.keep_connected();
            self.parked[slot] = Some(Parked { addr: r.addr.clone(), name: r.name.clone(), pointer: r.pointer.clone() });
        }
    }

    /// After the game, the parked remotes go back into their slots, their pointer as it was.
    fn unpark(&mut self) {
        let parked = std::mem::take(&mut self.parked);
        let Ok(bt) = self.bt.as_ref().map(Arc::clone) else { return };
        for (slot, p) in parked.into_iter().enumerate() {
            let Some(Parked { addr, name, pointer }) = p else { continue };
            self.places.insert(addr.clone(), slot);
            self.kept.insert(addr.clone(), pointer);
            self.connect(&bt, addr, name);
        }
    }

    /// Puts a remote lent to the game back to how the launcher uses it.
    fn take_back(&mut self, slot: usize) {
        let bar_top = self.bar_top;
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
        // What the gyroscope learned in game stays learned.
        r.pointer = lent.proxy.pointer;
        r.pointer.bar_top = bar_top;
        r.prepare(slot, probe);
    }

    fn open(&mut self, slot: usize, mut stream: Box<dyn Stream>) {
        use tokio::io::AsyncWriteExt;
        // The MotionPlus has to be sorted out first, and set_up done: Dolphin will ask again.
        let remote = self.slots.get_mut(slot).and_then(Option::as_mut).filter(|r| r.settled() && r.serving.is_none());
        let Some(r) = remote.filter(|_| self.paused) else {
            spawn(async move {
                let _ = stream.write_all(&[0]).await;
            });
            return;
        };
        let motion_plus = r.motion_plus == MotionPlusState::Active;
        eprintln!(
            "wiimote: J{} lent to the game ({}, bar {})",
            slot + 1,
            match (motion_plus, r.pointer.gyro.ready()) {
                (false, _) => "no MotionPlus",
                (true, true) => "gyroscope ready",
                (true, false) => "gyroscope not calibrated yet",
            },
            if r.pointer.bar_top { "on top" } else { "below" },
        );
        let proxy = Proxy::new(motion_plus, r.pointer.clone());
        let (to_game, from_remote) = mpsc::unbounded_channel();
        let (to_remote, mut writes) = mpsc::unbounded_channel::<Vec<u8>>();
        if let Some(first) = proxy.start() {
            let _ = to_remote.send(first);
        }
        *r.lent.lock().unwrap() = Some(Lent { proxy, to_game, to_remote });
        // Ends once the remote is taken back and what was queued is sent.
        let link = r.link.clone();
        spawn(async move {
            while let Some(report) = writes.recv().await {
                let _ = link.send(&report).await;
            }
        });
        let (lent, id, tx) = (r.lent.clone(), r.id, self.tx.clone());
        r.serving = Some(spawn(async move {
            serve(stream, lent, from_remote).await;
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
            slots: std::array::from_fn(|i| match (&self.slots[i], &self.parked[i]) {
                (Some(r), _) => Some(RemoteView {
                    addr: r.addr.clone(),
                    model: protocol::model(&r.name),
                    battery: r.battery,
                    extension: r.extension.map(protocol::extension_label),
                    motion_plus: r.motion_plus == MotionPlusState::Active,
                    buttons: protocol::button_names(r.buttons),
                }),
                // Still in the game, in its slot, though the launcher doesn't hear it.
                (None, Some(p)) => Some(RemoteView {
                    addr: p.addr.clone(),
                    model: protocol::model(&p.name),
                    battery: None,
                    extension: None,
                    motion_plus: false,
                    buttons: Vec::new(),
                }),
                (None, None) => None,
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
        .chain([protocol::report_mode(), protocol::read_memory(protocol::ACCEL_CAL, 10)])
        .chain(probe.into_iter().filter(|_| probe_motion_plus))
        .chain([protocol::status_request()]);
    for report in reports {
        let _ = link.send(&report).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(not(any(unix, windows)))]
fn listen(_: mpsc::UnboundedSender<Msg>) -> Option<(PathBuf, JoinHandle<()>)> {
    None
}

/// How long Dolphin has, once connected, to say which slot it wants.
const ASKING_FOR: Duration = Duration::from_secs(2);

/// Reads the slot Dolphin asks for on a new connection, then hands the connection over.
async fn ask(mut stream: impl Stream + 'static, tx: mpsc::UnboundedSender<Msg>) {
    use tokio::io::AsyncReadExt;
    let asked = tokio::time::timeout(ASKING_FOR, stream.read_u8()).await;
    if let Ok(Ok(slot)) = asked {
        let _ = tx.send(Msg::Open { slot: slot as usize, stream: Box::new(stream) });
    }
}

/// Opens the socket Dolphin asks for remotes through.
#[cfg(unix)]
fn listen(tx: mpsc::UnboundedSender<Msg>) -> Option<(PathBuf, JoinHandle<()>)> {
    use std::os::unix::fs::PermissionsExt;

    let bind = |dir: PathBuf| -> std::io::Result<(PathBuf, tokio::net::UnixListener)> {
        let path = dir.join(format!("wp-launcher-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = tokio::net::UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        Ok((path, listener))
    };
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let bound = runtime.into_iter().chain([std::env::temp_dir()]).find_map(|dir| {
        bind(dir.clone()).map_err(|e| eprintln!("wiimote: no socket for Dolphin in {}: {e}", dir.display())).ok()
    });
    let Some((path, listener)) = bound else {
        eprintln!("wiimote: no socket for Dolphin at all: it will have to connect the remotes itself");
        return None;
    };
    let task = spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            spawn(ask(stream, tx.clone()));
        }
    });
    Some((path, task))
}

/// Opens the named pipe Dolphin asks for remotes through: one instance per connection,
/// as many as Dolphin opens, local clients only.
#[cfg(windows)]
fn listen(tx: mpsc::UnboundedSender<Msg>) -> Option<(PathBuf, JoinHandle<()>)> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let name = format!(r"\\.\pipe\wp-launcher-{}", std::process::id());
    let path = PathBuf::from(&name);
    let create = move |first: bool| ServerOptions::new().first_pipe_instance(first).reject_remote_clients(true).create(&name);
    let mut pending = match create(true) {
        Ok(pipe) => pipe,
        Err(e) => {
            eprintln!("wiimote: no pipe for Dolphin ({e}): it will have to connect the remotes itself");
            return None;
        }
    };
    let task = spawn(async move {
        loop {
            let connected = pending.connect().await;
            // The next instance before this one is handed over, so the next slot can connect.
            let next = match create(false) {
                Ok(pipe) => pipe,
                Err(e) => {
                    eprintln!("wiimote: the pipe for Dolphin takes no more connections: {e}");
                    break;
                }
            };
            let pipe = std::mem::replace(&mut pending, next);
            if connected.is_ok() {
                spawn(ask(pipe, tx.clone()));
            }
        }
    });
    Some((path, task))
}

/// Carries one remote's reports between Dolphin and the proxy until Dolphin lets go.
/// Each is preceded by its length.
async fn serve(stream: Box<dyn Stream>, lent: Arc<Mutex<Option<Lent>>>, mut from_remote: mpsc::UnboundedReceiver<Vec<u8>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (mut read, mut write) = tokio::io::split(stream);
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
        let mut lent = lent.lock().unwrap();
        let Some(lent) = lent.as_mut() else { break };
        let forward = lent.proxy.from_game(report);
        for r in forward.to_game {
            let _ = lent.to_game.send(r);
        }
        // Queued under the lock, so the reader's can't overtake them (nor the other way).
        for r in forward.to_remote {
            let _ = lent.to_remote.send(r);
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
