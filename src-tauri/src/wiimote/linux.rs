//! Linux transport: BlueZ (D-Bus) finds the remotes, then we open the two HID
//! L2CAP channels ourselves, as Dolphin does. A remote the system already holds
//! through its own HID driver can't be used until it is released.

use super::protocol::is_wiimote_name;
use crate::i18n::tr;
use super::Availability;
use bluer::l2cap::{SeqPacket, SocketAddr};
use bluer::{Adapter, AddressType, Session};
use futures_util::StreamExt;
use std::time::Duration;

const PSM_CONTROL: u16 = 0x11;
const PSM_INTERRUPT: u16 = 0x13;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Bluetooth {
    _session: Session,
    adapter: Adapter,
}

/// A remote BlueZ knows about.
pub struct Seen {
    pub addr: String,
    pub name: String,
    /// Answered the current inquiry, i.e. it is in 1+2 / sync mode right now.
    pub in_range: bool,
    /// Has a Bluetooth link open (ours, or the system's HID driver).
    pub connected: bool,
}

/// Keeps BlueZ discovering while alive.
pub struct Discovery(tauri::async_runtime::JoinHandle<()>);

impl Drop for Discovery {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub struct Link {
    // Kept open for the lifetime of the link; HID traffic all goes over interrupt.
    _control: SeqPacket,
    interrupt: SeqPacket,
}

impl Link {
    pub async fn send(&self, report: &[u8]) -> Result<(), String> {
        self.interrupt.send(report).await.map(|_| ()).map_err(|e| e.to_string())
    }

    pub async fn recv(&self, buf: &mut [u8]) -> Result<usize, String> {
        match self.interrupt.recv(buf).await {
            Ok(0) => Err(tr!("déconnectée")),
            Ok(n) => Ok(n),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl Bluetooth {
    pub async fn open() -> Result<Self, Availability> {
        let session = Session::new()
            .await
            .map_err(|e| Availability::Error(tr!("Service Bluetooth injoignable ({e})", e = e)))?;
        let adapter = session.default_adapter().await.map_err(|_| Availability::NoAdapter)?;
        Ok(Bluetooth { _session: session, adapter })
    }

    pub async fn powered(&self) -> bool {
        self.adapter.is_powered().await.unwrap_or(false)
    }

    pub async fn discover(&self) -> Result<Discovery, String> {
        let mut events = self.adapter.discover_devices().await.map_err(|e| e.to_string())?;
        Ok(Discovery(tauri::async_runtime::spawn(async move { while events.next().await.is_some() {} })))
    }

    pub async fn remotes(&self) -> Vec<Seen> {
        let mut out = Vec::new();
        for addr in self.adapter.device_addresses().await.unwrap_or_default() {
            let Ok(dev) = self.adapter.device(addr) else { continue };
            let Some(name) = dev.name().await.ok().flatten().filter(|n| is_wiimote_name(n)) else { continue };
            out.push(Seen {
                addr: addr.to_string(),
                name,
                in_range: dev.rssi().await.ok().flatten().is_some(),
                connected: dev.is_connected().await.unwrap_or(false),
            });
        }
        out
    }

    pub async fn connect(&self, addr: &str) -> Result<Link, String> {
        let addr = addr.parse().map_err(|_| tr!("adresse invalide : {addr}", addr = addr))?;
        let open = |psm| async move {
            tokio::time::timeout(CONNECT_TIMEOUT, SeqPacket::connect(SocketAddr::new(addr, AddressType::BrEdr, psm)))
                .await
                .map_err(|_| tr!("pas de réponse"))?
                .map_err(|e| e.to_string())
        };
        let control = open(PSM_CONTROL).await?;
        let interrupt = open(PSM_INTERRUPT).await?;
        Ok(Link { _control: control, interrupt })
    }

    /// Asks BlueZ to drop its own connection to a remote (it then switches off).
    pub async fn release(&self, addr: &str) -> Result<(), String> {
        let addr = addr.parse().map_err(|_| tr!("adresse invalide : {addr}", addr = addr))?;
        let dev = self.adapter.device(addr).map_err(|e| e.to_string())?;
        dev.disconnect().await.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs bluetoothd: lists the Wii Remotes BlueZ knows, without touching them.
    #[test]
    #[ignore]
    fn lists_known_remotes() {
        tauri::async_runtime::block_on(async {
            let Ok(bt) = Bluetooth::open().await else { panic!("no Bluetooth") };
            println!("powered: {}", bt.powered().await);
            for r in bt.remotes().await {
                println!("{} {} in_range={} connected={}", r.addr, r.name, r.in_range, r.connected);
            }
        });
    }
}
