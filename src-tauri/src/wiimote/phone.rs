//! Phones as Wii Remotes. The launcher serves a page (phone.html, src/phone/) on the local
//! network; a phone that opens it becomes a Wii Remote with a MotionPlus (emulated.rs): its
//! buttons on the screen, its accelerometer and gyroscope for the remote's, its vibrator for
//! the rumble, its speaker for the remote's.
//!
//! Browsers only give a page the phone's motion over HTTPS: the launcher makes its own
//! certificate, which the phone is asked to accept once. A token in the page's address
//! (from the QR code) keeps the rest of the network from connecting.

use super::emulated::{Emulated, Event};
use super::{journal, Wiimotes};
use crate::i18n::tr;
use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tauri::async_runtime::{spawn, JoinHandle};
use tauri::AppHandle;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

/// The name a phone's remote goes by (protocol::model).
pub const NAME: &str = "Phone";
/// Ports tried in turn: the first that is free.
const PORTS: std::ops::RangeInclusive<u16> = 8730..=8739;
/// The remote reports this often, as one on a PC's Bluetooth.
const REPORT_EVERY: Duration = Duration::from_millis(10);
/// A phone that says nothing for this long is gone (locked, out of range): it gets its
/// slot back when it comes back.
const SILENT_FOR: Duration = Duration::from_secs(3);
/// iOS refuses certificates valid for longer (and without the server's usage).
const CERT_DAYS: i64 = 800;
const HEAD_LIMIT: usize = 16 * 1024;
/// The page's development server (tauri.conf.json's devUrl). By name: Vite may only listen on
/// ::1, and connecting tries each address the name has.
const DEV_SERVER: &str = "localhost:1420";

/// What the remote's owner, the manager, holds: the phone's end of it.
pub struct Link {
    shared: Arc<Mutex<Shared>>,
    reports: tokio::sync::Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    replies: mpsc::UnboundedSender<Vec<u8>>,
    to_phone: mpsc::UnboundedSender<ToPhone>,
    gone: watch::Receiver<bool>,
}

#[derive(Default)]
struct Shared {
    remote: Emulated,
    /// The phone's battery, when its browser tells.
    battery: Option<u8>,
}

enum ToPhone {
    Event(Event),
    /// Why the remote is let go: "full", "stopped".
    Close(&'static str),
}

impl Link {
    pub async fn send(&self, report: &[u8]) -> Result<(), String> {
        if *self.gone.borrow() {
            return Err(tr!("déconnectée"));
        }
        let mut events = Vec::new();
        let replies = self.shared.lock().unwrap().remote.output(report, &mut events);
        for r in replies {
            let _ = self.replies.send(r);
        }
        for e in events {
            let _ = self.to_phone.send(ToPhone::Event(e));
        }
        Ok(())
    }

    pub async fn recv(&self, buf: &mut [u8]) -> Result<usize, String> {
        let mut reports = self.reports.lock().await;
        let mut gone = self.gone.clone();
        tokio::select! {
            r = reports.recv() => {
                let r = r.ok_or_else(|| tr!("déconnectée"))?;
                let n = r.len().min(buf.len());
                buf[..n].copy_from_slice(&r[..n]);
                Ok(n)
            }
            _ = gone.wait_for(|&g| g) => Err(tr!("déconnectée")),
        }
    }

    pub fn battery(&self) -> Option<u8> {
        self.shared.lock().unwrap().battery
    }

    /// Tells the phone why it isn't kept: it goes once the link is dropped.
    pub fn close(&self, why: &'static str) {
        let _ = self.to_phone.send(ToPhone::Close(why));
    }
}

/// The server, while phones can connect.
pub struct Server {
    task: JoinHandle<()>,
    pub url: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The page's address for a phone, and its QR code as an SVG path (one unit per module).
pub fn qr_path(url: &str) -> (String, usize) {
    let Ok(code) = qrcode::QrCode::new(url.as_bytes()) else { return (String::new(), 0) };
    let width = code.width();
    let mut path = String::new();
    for (i, c) in code.to_colors().iter().enumerate() {
        if *c == qrcode::Color::Dark {
            path.push_str(&format!("M{} {}h1v1h-1z", i % width, i / width));
        }
    }
    (path, width)
}

/// The page's files: bytes and type for a path. None: in development, from Vite's server.
type Assets = Option<Arc<dyn Fn(&str) -> Option<(Vec<u8>, String)> + Send + Sync>>;

pub async fn start(app: &AppHandle, wiimotes: Wiimotes) -> Result<Server, String> {
    let dir = crate::game::data_dir(app, "phone")?;
    let app = app.clone();
    let assets: Assets = (!tauri::is_dev())
        .then(|| Arc::new(move |path: &str| app.asset_resolver().get(path.to_string()).map(|a| (a.bytes, a.mime_type))) as _);
    listen(&dir, assets, wiimotes).await
}

async fn listen(dir: &Path, assets: Assets, wiimotes: Wiimotes) -> Result<Server, String> {
    let ip = lan_ip().ok_or_else(|| tr!("Pas de réseau local : connecte l’ordinateur au Wi-Fi ou par câble."))?;
    let token = token(dir)?;
    let tls = tls(dir, ip)?;
    let mut bound = None;
    for port in PORTS {
        if let Ok(l) = TcpListener::bind(SocketAddr::new(IpAddr::from([0, 0, 0, 0]), port)).await {
            bound = Some((l, port));
            break;
        }
    }
    let (listener, port) = bound.ok_or_else(|| tr!("Aucun port libre pour les téléphones ({from}–{to})", from = PORTS.start(), to = PORTS.end()))?;
    let url = format!("https://{ip}:{port}/#{token}");
    journal::note!("phones can connect at https://{ip}:{port}/");
    let task = spawn(async move {
        let token = Arc::new(token);
        while let Ok((stream, from)) = listener.accept().await {
            let (tls, assets, token, wiimotes) = (tls.clone(), assets.clone(), token.clone(), wiimotes.clone());
            spawn(async move {
                let Ok(Ok(stream)) = tokio::time::timeout(Duration::from_secs(10), tls.accept(stream)).await else { return };
                serve(stream, from.ip(), &assets, &token, wiimotes).await;
            });
        }
    });
    Ok(Server { task, url })
}

/// The computer's address on the local network: the one its default route leaves from
/// (connecting a UDP socket sends nothing).
fn lan_ip() -> Option<IpAddr> {
    ["8.8.8.8:80", "192.168.1.1:80", "10.0.0.1:80"].iter().find_map(|to| {
        let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect(to).ok()?;
        let ip = socket.local_addr().ok()?.ip();
        (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
    })
}

/// The pairing token, kept so that a phone's bookmark keeps working.
fn token(dir: &Path) -> Result<String, String> {
    let file = dir.join("token");
    if let Ok(t) = std::fs::read_to_string(&file) {
        if t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(t);
        }
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    let t: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::write(&file, &t).map_err(|e| e.to_string())?;
    Ok(t)
}

/// The launcher's own certificate, kept (a phone that accepted it isn't asked again) until
/// it nears its end or the computer's address changes.
fn tls(dir: &Path, ip: IpAddr) -> Result<tokio_rustls::TlsAcceptor, String> {
    let (cert_file, key_file, ip_file) = (dir.join("cert.der"), dir.join("key.der"), dir.join("cert-ip"));
    let fresh = std::fs::metadata(&cert_file)
        .and_then(|m| m.modified())
        .is_ok_and(|t| SystemTime::now().duration_since(t).is_ok_and(|age| age < Duration::from_secs(86_400 * (CERT_DAYS as u64 - 30))));
    let same_ip = std::fs::read_to_string(&ip_file).is_ok_and(|s| s == ip.to_string());
    let kept = (fresh && same_ip).then(|| Some((std::fs::read(&cert_file).ok()?, std::fs::read(&key_file).ok()?))).flatten();
    let (cert, key) = match kept {
        Some(pair) => pair,
        None => {
            let mut params = rcgen::CertificateParams::new(vec!["wp-launcher.local".to_string()]).map_err(|e| e.to_string())?;
            params.subject_alt_names.push(rcgen::SanType::IpAddress(ip));
            params.distinguished_name.push(rcgen::DnType::CommonName, "Wii Party Launcher");
            params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
            let now = time::OffsetDateTime::now_utc();
            params.not_before = now - time::Duration::days(1);
            params.not_after = now + time::Duration::days(CERT_DAYS);
            let key = rcgen::KeyPair::generate().map_err(|e| e.to_string())?;
            let cert = params.self_signed(&key).map_err(|e| e.to_string())?;
            let pair = (cert.der().to_vec(), key.serialize_der());
            let _ = std::fs::write(&cert_file, &pair.0);
            let _ = std::fs::write(&key_file, &pair.1);
            let _ = std::fs::write(&ip_file, ip.to_string());
            pair
        }
    };
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(cert)], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)))
        .map_err(|e| e.to_string())?;
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(config)))
}

/// One request's head: method, target, headers (names in lower case), and what came after it.
struct Head {
    raw: Vec<u8>,
    target: String,
    headers: Vec<(String, String)>,
    rest: Vec<u8>,
}

impl Head {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    fn path(&self) -> &str {
        self.target.split(['?', '#']).next().unwrap_or("/")
    }

    fn query(&self, name: &str) -> Option<&str> {
        let query = self.target.split_once('?')?.1;
        query.split('&').find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
    }
}

async fn read_head(stream: &mut (impl AsyncRead + Unpin)) -> Option<Head> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    let end = loop {
        if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        if buf.len() > HEAD_LIMIT {
            return None;
        }
        let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk)).await.ok()?.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let text = std::str::from_utf8(&buf[..end]).ok()?;
    let mut lines = text.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target) = (first.next()?, first.next()?.to_string());
    if method != "GET" {
        return None;
    }
    let headers = lines.filter_map(|l| l.split_once(':')).map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string())).collect();
    let rest = buf[end..].to_vec();
    buf.truncate(end);
    Some(Head { raw: buf, target, headers, rest })
}

async fn respond(stream: &mut (impl AsyncWrite + Unpin), status: &str, kind: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes()).await;
    let _ = stream.write_all(body).await;
    let _ = stream.shutdown().await;
}

async fn serve<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(mut stream: S, from: IpAddr, assets: &Assets, token: &str, wiimotes: Wiimotes) {
    let Some(head) = read_head(&mut stream).await else { return };
    let upgrade = head.header("upgrade").is_some_and(|u| u.eq_ignore_ascii_case("websocket"));
    if head.path() == "/remote" && upgrade {
        let (Some(key), true) = (head.header("sec-websocket-key"), head.query("t") == Some(token)) else {
            journal::note!("phone {from} refused: wrong token");
            return respond(&mut stream, "403 Forbidden", "text/plain", b"").await;
        };
        let accept = derive_accept_key(key.as_bytes());
        let answer = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n");
        if stream.write_all(answer.as_bytes()).await.is_err() {
            return;
        }
        let ws = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
        return session(ws, from, wiimotes).await;
    }
    let Some(assets) = assets else {
        return proxy(stream, head).await;
    };
    // The page and its files, from the launcher's own.
    let path = match head.path() {
        "/" => "/phone.html",
        p => p,
    };
    match assets(path) {
        Some((bytes, kind)) => respond(&mut stream, "200 OK", &kind, &bytes).await,
        None => respond(&mut stream, "404 Not Found", "text/plain", b"").await,
    }
}

/// In development the page comes from Vite's server, live (its own socket for reloading too).
async fn proxy<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S, head: Head) {
    let Ok(mut upstream) = TcpStream::connect(DEV_SERVER).await else {
        return respond(&mut stream, "502 Bad Gateway", "text/plain; charset=utf-8", "Vite ne répond pas sur localhost:1420 (npm run dev)".as_bytes()).await;
    };
    let mut raw = head.raw.clone();
    if head.path() == "/" && head.header("upgrade").is_none() {
        let text = String::from_utf8_lossy(&raw).replacen(&format!("GET {} ", head.target), "GET /phone.html ", 1);
        raw = text.into_bytes();
    }
    raw.extend_from_slice(&head.rest);
    if upstream.write_all(&raw).await.is_ok() {
        let _ = tokio::io::copy_bidirectional(&mut stream, &mut upstream).await;
    }
}

/// One phone, from its socket opening to its closing: its remote in a slot, its motion and
/// buttons in, the remote's reports out at 100 Hz, and back to it the rumble, LEDs and sound.
async fn session<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(ws: WebSocketStream<S>, from: IpAddr, wiimotes: Wiimotes) {
    let (mut sink, mut stream) = ws.split();
    let shared = Arc::new(Mutex::new(Shared::default()));
    let (replies, reports) = mpsc::unbounded_channel();
    let (to_phone, mut events) = mpsc::unbounded_channel();
    let (gone_tx, gone) = watch::channel(false);
    let link = Link { shared: shared.clone(), reports: tokio::sync::Mutex::new(reports), replies: replies.clone(), to_phone, gone };
    journal::note!("phone {from} connected");
    wiimotes.adopt_phone(from.to_string(), link);

    let mut tick = tokio::time::interval(REPORT_EVERY);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut heard = tokio::time::Instant::now();
    let why = loop {
        tokio::select! {
            msg = stream.next() => {
                heard = tokio::time::Instant::now();
                match msg {
                    Some(Ok(Message::Binary(b))) => {
                        if let Some(sensors) = read_sensors(&b) {
                            shared.lock().unwrap().remote.sensors = sensors;
                        }
                    }
                    Some(Ok(Message::Text(t))) => {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
                            if let Some(level) = v.get("battery") {
                                let mut s = shared.lock().unwrap();
                                s.battery = level.as_f64().map(|l| l.clamp(0.0, 100.0) as u8);
                                s.remote.battery = s.battery.map_or(0xC8, |l| (l as u32 * 0xC8 / 100) as u8);
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break "closed",
                    Some(Ok(_)) => {}
                }
            }
            event = events.recv() => {
                let message = match event {
                    // The manager dropped the remote: "Déconnecter", or launcher stopping.
                    None => break "removed",
                    Some(ToPhone::Close(why)) => break why,
                    Some(ToPhone::Event(Event::Rumble(on))) => Message::text(format!(r#"{{"rumble":{on}}}"#)),
                    Some(ToPhone::Event(Event::Leds(leds))) => Message::text(format!(r#"{{"leds":{leds}}}"#)),
                    Some(ToPhone::Event(Event::Sound { rate, volume, samples })) => {
                        let mut bytes = Vec::with_capacity(8 + samples.len() * 2);
                        bytes.extend(rate.to_le_bytes());
                        bytes.extend(volume.to_le_bytes());
                        bytes.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
                        Message::binary(bytes)
                    }
                };
                if sink.send(message).await.is_err() {
                    break "closed";
                }
            }
            _ = tick.tick() => {
                if heard.elapsed() > SILENT_FOR {
                    break "silent";
                }
                let report = shared.lock().unwrap().remote.tick();
                if let Some(report) = report {
                    if replies.send(report).is_err() {
                        break "removed";
                    }
                }
            }
        }
    };
    journal::note!("phone {from} gone ({why})");
    let _ = gone_tx.send(true);
    if why != "closed" {
        let _ = sink.send(Message::text(format!(r#"{{"bye":"{why}"}}"#))).await;
        let _ = sink.close().await;
    }
}

/// A phone's message: buttons, then the accelerometer (g) and gyroscope (°/s) along the
/// remote's axes, little-endian 32-bit floats.
fn read_sensors(b: &[u8]) -> Option<super::emulated::Sensors> {
    let f: Vec<f32> = b.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    if f.len() < 7 || f.iter().any(|v| !v.is_finite()) {
        return None;
    }
    Some(super::emulated::Sensors { buttons: f[0] as u16 & 0x1F9F, accel: [f[1], f[2], f[3]], gyro: [f[4], f[5], f[6]] })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_phones_message() {
        let bytes: Vec<u8> = [8.0f32, 0.0, 0.0, 1.0, 1.5, -2.0, 30.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        let s = read_sensors(&bytes).unwrap();
        assert_eq!(s.buttons, 8);
        assert_eq!(s.accel, [0.0, 0.0, 1.0]);
        assert_eq!(s.gyro, [1.5, -2.0, 30.0]);
        assert!(read_sensors(&bytes[..20]).is_none());
        let nan: Vec<u8> = [0.0f32, f32::NAN, 0.0, 1.0, 0.0, 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert!(read_sensors(&nan).is_none());
    }

    #[test]
    fn the_qr_code_draws_the_address() {
        let (path, width) = qr_path("https://192.168.1.20:8730/#0123456789abcdef0123456789abcdef");
        assert!(width >= 21 && path.starts_with("M0 0h1v1h-1z"));
    }

    /// The real server with a stand-in manager, for a browser to try (WP_PHONE_FOR seconds):
    /// `cargo test a_phone_in_a_browser -- --ignored --nocapture`. It sets the remote up as
    /// the launcher does, then prints what it reports, and rumbles and beeps now and then.
    #[tokio::test]
    #[ignore]
    async fn a_phone_in_a_browser() {
        use super::super::{protocol, Msg};
        let dir = std::env::temp_dir().join("wp-phone-test");
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        // The built page (npm run build), as a release serves it.
        let dist = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist");
        let assets: Assets = Some(Arc::new(move |path: &str| {
            let file = dist.join(path.trim_start_matches('/'));
            let kind = match file.extension().and_then(|e| e.to_str()) {
                Some("html") => "text/html",
                Some("js") => "text/javascript",
                Some("css") => "text/css",
                Some("woff2") => "font/woff2",
                _ => "application/octet-stream",
            };
            std::fs::read(&file).ok().map(|b| (b, kind.to_string()))
        }));
        let server = listen(&dir, assets, Wiimotes(tx)).await.unwrap();
        println!("URL {}", server.url);
        let seconds = std::env::var("WP_PHONE_FOR").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
        let run = async {
            while let Some(msg) = rx.recv().await {
                let Msg::Connected { addr, link: Ok(super::super::Link::Phone(link)), .. } = msg else { continue };
                println!("phone {addr} connected");
                spawn(async move {
                    let (addr, value) = protocol::MOTION_PLUS_INIT;
                    let (start, on) = protocol::MOTION_PLUS_START;
                    for r in [protocol::leds(1), protocol::write_register(addr, &[value]), protocol::write_register(start, &[on]), protocol::report_mode()] {
                        link.send(&r).await.unwrap();
                    }
                    // A beep: the speaker set up as Wii Party does, then ADPCM data.
                    link.send(&[protocol::OUT, 0x14, 0x04]).await.unwrap();
                    link.send(&protocol::write_register(0xA2_0001, &[0x00, 0x00, 0xD0, 0x07, 0x40, 0x00, 0x00])).await.unwrap();
                    let mut buf = [0u8; 32];
                    let mut n = 0u32;
                    loop {
                        let Ok(len) = link.recv(&mut buf).await else {
                            println!("phone gone");
                            break;
                        };
                        n += 1;
                        if n % 50 == 0 {
                            if let protocol::Input::Data { buttons, ext, .. } = protocol::parse(&buf[..len]) {
                                let accel = super::super::pointer::accel(&buf[..len]);
                                println!("buttons {:?} accel {accel:?} gyro {:?}", protocol::button_names(buttons), protocol::motion_plus(&ext).map(|m| m.raw));
                            }
                        }
                        if n % 300 == 0 {
                            link.send(&protocol::rumble(true)).await.unwrap();
                            for _ in 0..20 {
                                let mut beep = vec![protocol::OUT, 0x18, 20 << 3];
                                beep.extend([0x7F, 0x08, 0x7F, 0x08].repeat(5));
                                link.send(&beep).await.unwrap();
                            }
                        }
                        if n % 300 == 40 {
                            link.send(&protocol::rumble(false)).await.unwrap();
                        }
                    }
                });
            }
        };
        let _ = tokio::time::timeout(Duration::from_secs(seconds), run).await;
    }

    #[tokio::test]
    async fn reads_a_request_head() {
        let mut req: &[u8] = b"GET /remote?t=abc&x=1 HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\n\r\nrest";
        let head = read_head(&mut req).await.unwrap();
        assert_eq!(head.path(), "/remote");
        assert_eq!(head.query("t"), Some("abc"));
        assert_eq!(head.header("upgrade"), Some("websocket"));
        assert_eq!(head.rest, b"rest");
        let mut post: &[u8] = b"POST / HTTP/1.1\r\n\r\n";
        assert!(read_head(&mut post).await.is_none());
    }
}
