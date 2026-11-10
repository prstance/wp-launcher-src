//! Windows transport: the Bluetooth API finds the remotes and has Windows connect
//! them (HID service enabled, no pairing, as Dolphin does), then we talk to each
//! through the HID device Windows makes of it. Windows keeps a remote's HID device
//! around after it is gone, so whether it is connected comes from Bluetooth.

use super::journal;
use super::protocol::{self, is_wiimote_name};
use super::Availability;
use crate::i18n::tr;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::async_runtime::spawn_blocking;
use tokio::sync::{mpsc, Mutex};
use windows_sys::core::GUID;
use windows_sys::Win32::Devices::Bluetooth::{
    BluetoothFindDeviceClose, BluetoothFindFirstDevice, BluetoothFindFirstRadio, BluetoothFindNextDevice,
    BluetoothFindRadioClose, BluetoothGetDeviceInfo, BluetoothIsConnectable, BluetoothRemoveDevice,
    BluetoothSetServiceState, BLUETOOTH_DEVICE_INFO, BLUETOOTH_DEVICE_SEARCH_PARAMS, BLUETOOTH_FIND_RADIO_PARAMS,
    BLUETOOTH_SERVICE_ENABLE,
};
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_Device_IDW, CM_Get_Device_Interface_ListW, CM_Get_Device_Interface_List_SizeW,
    CM_Get_Device_Interface_PropertyW, CM_Get_Parent, CM_Locate_DevNodeW, CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
    CM_LOCATE_DEVNODE_NORMAL, CR_SUCCESS, MAX_DEVICE_ID_LEN,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::GUID_DEVINTERFACE_HID;
use windows_sys::Win32::Devices::Properties::DEVPKEY_Device_InstanceId;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows_sys::Win32::System::IO::{CancelIoEx, DeviceIoControl, GetOverlappedResult, OVERLAPPED};

/// The Bluetooth HID profile.
const HID_SERVICE: GUID = GUID::from_u128(0x00001124_0000_1000_8000_00805f9b34fb);
/// IOCTL_BTH_DISCONNECT_DEVICE (bthioctl.h): drops the link to a device, which switches a remote off.
const IOCTL_BTH_DISCONNECT_DEVICE: u32 = 0x0041_000C;
/// How long an inquiry lasts, in 1.28 s units (Dolphin's).
const INQUIRY: u8 = 3;
/// Nintendo's vendor id, as it appears in a Bluetooth HID device's instance id.
const NINTENDO: &str = "_VID&0002057E";
const WRITE_TIMEOUT_MS: u32 = 1000;
const READ_WAIT_MS: u32 = 200;
/// After this long without a report, a reader asks Bluetooth whether its remote is still there.
const CHECK_AFTER: Duration = Duration::from_secs(2);

/// A Windows handle closed when dropped.
struct Handle(HANDLE);

// Handles are plain kernel references: any thread can use or close one.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn last_error() -> String {
    std::io::Error::last_os_error().to_string()
}

fn radio() -> Option<Handle> {
    let params = BLUETOOTH_FIND_RADIO_PARAMS { dwSize: size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32 };
    let mut radio = null_mut();
    let find = unsafe { BluetoothFindFirstRadio(&params, &mut radio) };
    if find.is_null() {
        return None;
    }
    unsafe { BluetoothFindRadioClose(find) };
    Some(Handle(radio))
}

fn device_info(addr: u64) -> BLUETOOTH_DEVICE_INFO {
    let mut info = BLUETOOTH_DEVICE_INFO { dwSize: size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
    info.Address.Anonymous.ullLong = addr;
    info
}

fn name(info: &BLUETOOTH_DEVICE_INFO) -> String {
    let len = info.szName.iter().position(|&c| c == 0).unwrap_or(info.szName.len());
    String::from_utf16_lossy(&info.szName[..len])
}

fn address(info: &BLUETOOTH_DEVICE_INFO) -> u64 {
    unsafe { info.Address.Anonymous.ullLong }
}

fn format_addr(addr: u64) -> String {
    addr.to_be_bytes()[2..].iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

fn parse_addr(addr: &str) -> Result<u64, String> {
    let hex: String = addr.chars().filter(|&c| c != ':').collect();
    u64::from_str_radix(&hex, 16).ok().filter(|_| hex.len() == 12).ok_or_else(|| tr!("adresse invalide : {addr}", addr = addr))
}

/// The Wii Remotes the radio knows of, and with `inquiry` (blocks that many 1.28 s)
/// those that answer one, i.e. are in 1+2 / sync mode right now.
fn wiimotes(radio: &Handle, inquiry: u8) -> Vec<BLUETOOTH_DEVICE_INFO> {
    let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
        dwSize: size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
        fReturnAuthenticated: 1,
        fReturnRemembered: 1,
        fReturnUnknown: 1,
        fReturnConnected: 1,
        fIssueInquiry: (inquiry > 0).into(),
        cTimeoutMultiplier: inquiry,
        hRadio: radio.0,
    };
    let mut info = device_info(0);
    let find = unsafe { BluetoothFindFirstDevice(&params, &mut info) };
    if find.is_null() {
        return Vec::new();
    }
    let mut out = Vec::new();
    loop {
        if is_wiimote_name(&name(&info)) {
            out.push(info);
        }
        if unsafe { BluetoothFindNextDevice(find, &mut info) } == 0 {
            break;
        }
    }
    unsafe { BluetoothFindDeviceClose(find) };
    out
}

fn is_connected(addr: u64) -> bool {
    let Some(radio) = radio() else { return false };
    let mut info = device_info(addr);
    unsafe { BluetoothGetDeviceInfo(radio.0, &mut info) == 0 && info.fConnected != 0 }
}

fn disconnect(addr: u64) -> Result<(), String> {
    let radio = radio().ok_or_else(|| tr!("Bluetooth indisponible"))?;
    let mut returned = 0;
    let done = unsafe {
        DeviceIoControl(radio.0, IOCTL_BTH_DISCONNECT_DEVICE, (&addr as *const u64).cast(), 8, null_mut(), 0, &mut returned, null_mut())
    };
    if done == 0 {
        return Err(last_error());
    }
    Ok(())
}

/// The Wii Remote HID devices Windows has, with the Bluetooth address of each.
fn hid_devices() -> Vec<(u64, Vec<u16>)> {
    let mut list = Vec::new();
    for _ in 0..3 {
        let mut len = 0;
        let flags = CM_GET_DEVICE_INTERFACE_LIST_PRESENT;
        if unsafe { CM_Get_Device_Interface_List_SizeW(&mut len, &GUID_DEVINTERFACE_HID, null(), flags) } != CR_SUCCESS {
            return Vec::new();
        }
        list = vec![0u16; len as usize];
        // Fails if an interface appeared in between: ask for the size again.
        if unsafe { CM_Get_Device_Interface_ListW(&GUID_DEVINTERFACE_HID, null(), list.as_mut_ptr(), len, flags) } == CR_SUCCESS {
            break;
        }
        list.clear();
    }
    list.split(|&c| c == 0)
        .filter(|path| !path.is_empty())
        .filter_map(|path| {
            let path: Vec<u16> = path.iter().copied().chain([0]).collect();
            Some((bluetooth_address(&path)?, path))
        })
        .collect()
}

/// A Wii Remote's HID device hangs off the Bluetooth device whose instance id ends
/// in its address: BTHENUM\{…}_VID&0002057e_PID&0306\7&…&0&0017AB123456_C00000000.
fn bluetooth_address(path: &[u16]) -> Option<u64> {
    let mut id = [0u16; MAX_DEVICE_ID_LEN as usize + 1];
    let (mut kind, mut size) = (0, size_of_val(&id) as u32);
    let found = unsafe {
        CM_Get_Device_Interface_PropertyW(path.as_ptr(), &DEVPKEY_Device_InstanceId, &mut kind, id.as_mut_ptr().cast(), &mut size, 0)
    };
    if found != CR_SUCCESS || !string(&id).to_uppercase().contains(NINTENDO) {
        return None;
    }
    let (mut node, mut parent) = (0, 0);
    unsafe {
        if CM_Locate_DevNodeW(&mut node, id.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) != CR_SUCCESS
            || CM_Get_Parent(&mut parent, node, 0) != CR_SUCCESS
            || CM_Get_Device_IDW(parent, id.as_mut_ptr(), id.len() as u32, 0) != CR_SUCCESS
        {
            return None;
        }
    }
    let parent = string(&id);
    let last = parent.rsplit(['\\', '&']).next()?;
    let hex = last.split('_').next()?;
    u64::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 12)
}

fn string(wide: &[u16]) -> String {
    let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len])
}

/// Windows remembers a remote connected with 1+2 without pairing it, and then
/// can't connect it again: those are forgotten before looking for remotes.
fn forget_unusable(radio: &Handle) {
    for info in wiimotes(radio, 0) {
        if info.fRemembered != 0 && info.fConnected == 0 && info.fAuthenticated == 0 {
            unsafe { BluetoothRemoveDevice(&info.Address) };
        }
    }
}

fn event() -> Result<Handle, String> {
    let event = unsafe { CreateEventW(null(), 1, 0, null()) };
    if event.is_null() {
        return Err(last_error());
    }
    Ok(Handle(event))
}

/// Waits for an overlapped operation to finish, cancelling it if `give_up` says so.
fn finish(device: &Handle, overlapped: &OVERLAPPED, mut give_up: impl FnMut() -> bool) -> Result<u32, String> {
    while unsafe { WaitForSingleObject(overlapped.hEvent, READ_WAIT_MS) } != WAIT_OBJECT_0 {
        if give_up() {
            unsafe { CancelIoEx(device.0, overlapped) };
            break;
        }
    }
    // Once cancelled the operation still has to complete before `overlapped` goes away.
    let mut done = 0;
    if unsafe { GetOverlappedResult(device.0, overlapped, &mut done, 1) } == 0 {
        return Err(last_error());
    }
    Ok(done)
}

fn write(device: &Handle, report: &[u8]) -> Result<(), String> {
    let event = event()?;
    let mut overlapped = OVERLAPPED { hEvent: event.0, ..Default::default() };
    let mut written = 0;
    if unsafe { WriteFile(device.0, report.as_ptr(), report.len() as u32, &mut written, &mut overlapped) } != 0 {
        return Ok(());
    }
    if unsafe { GetLastError() } != ERROR_IO_PENDING {
        return Err(last_error());
    }
    let started = Instant::now();
    finish(device, &overlapped, || started.elapsed() >= Duration::from_millis(WRITE_TIMEOUT_MS.into())).map(|_| ())
}

/// Reads a remote's input reports until the link is dropped or the remote is gone.
fn read(device: Arc<Handle>, addr: u64, stop: Arc<AtomicBool>, reports: mpsc::UnboundedSender<Result<Vec<u8>, String>>) {
    let end = |e: String| {
        let _ = reports.send(Err(e));
    };
    let event = match event() {
        Ok(event) => event,
        Err(e) => return end(e),
    };
    // One report at a time, its id then 21 bytes, as Dolphin reads (MAX_PAYLOAD - 1): a
    // bigger buffer can be handed several queued reports at once, of which only the first
    // was looked at (MotionPlus answers and data lost).
    let mut buf = [0u8; 22];
    let mut odd_sizes = 0;
    let mut last_heard = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let mut overlapped = OVERLAPPED { hEvent: event.0, ..Default::default() };
        let mut n = 0;
        let read = unsafe { ReadFile(device.0, buf.as_mut_ptr(), buf.len() as u32, &mut n, &mut overlapped) };
        if read == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
            return end(last_error());
        }
        let mut gone = false;
        let result = finish(&device, &overlapped, || {
            if stop.load(Ordering::Relaxed) {
                return true;
            }
            if last_heard.elapsed() >= CHECK_AFTER {
                last_heard = Instant::now();
                gone = !is_connected(addr);
            }
            gone
        });
        if stop.load(Ordering::Relaxed) {
            return;
        }
        if gone {
            return end(tr!("déconnectée"));
        }
        let n = match result {
            Ok(n) => n as usize,
            Err(e) => return end(e),
        };
        last_heard = Instant::now();
        if n != buf.len() && odd_sizes < 5 {
            odd_sizes += 1;
            journal::note!("{} read {n} bytes: {}", format_addr(addr), journal::hex(&buf[..n]));
        }
        // As it comes over L2CAP: header, then only the report's own bytes.
        if reports.send(Ok(protocol::from_hid(&buf[..n]))).is_err() {
            return;
        }
    }
}

pub struct Bluetooth;

/// A remote Windows knows about.
pub struct Seen {
    pub addr: String,
    pub name: String,
    /// Connected to Windows, which has made it a HID device we can open.
    pub in_range: bool,
    /// Always false: the HID device is shared, nothing holds a remote away from us.
    pub connected: bool,
}

/// Keeps inquiring, and has Windows connect the remotes that answer, while alive.
pub struct Discovery(Arc<AtomicBool>);

impl Drop for Discovery {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub struct Link {
    device: Arc<Handle>,
    addr: u64,
    reports: Mutex<mpsc::UnboundedReceiver<Result<Vec<u8>, String>>>,
    /// A failed write to a remote Bluetooth no longer has ends the link too.
    lost: mpsc::UnboundedSender<Result<Vec<u8>, String>>,
    /// One write at a time, in the order they're sent.
    writing: Mutex<()>,
    stop: Arc<AtomicBool>,
    /// Closing the HID device leaves the remote connected (Dolphin has it).
    keep: AtomicBool,
}

impl Link {
    pub async fn send(&self, report: &[u8]) -> Result<(), String> {
        // Windows adds the HID transaction header itself.
        let report = protocol::to_hid(report).to_vec();
        let sent = journal::hex(&report);
        let device = self.device.clone();
        let _turn = self.writing.lock().await;
        let result = spawn_blocking(move || write(&device, &report)).await.map_err(|e| e.to_string())?;
        if let Err(e) = &result {
            journal::note!("{} write {sent} failed: {e}", format_addr(self.addr));
            // One write that timed out isn't a remote gone: Bluetooth tells.
            let addr = self.addr;
            if !spawn_blocking(move || is_connected(addr)).await.unwrap_or(true) {
                let _ = self.lost.send(Err(e.clone()));
            }
        }
        result
    }

    pub fn keep_connected(&self) {
        self.keep.store(true, Ordering::Relaxed);
    }

    pub async fn recv(&self, buf: &mut [u8]) -> Result<usize, String> {
        match self.reports.lock().await.recv().await {
            Some(Ok(report)) => {
                let n = report.len().min(buf.len());
                buf[..n].copy_from_slice(&report[..n]);
                Ok(n)
            }
            Some(Err(e)) => Err(e),
            None => Err(tr!("déconnectée")),
        }
    }
}

impl Drop for Link {
    // Closing the HID device leaves the remote connected to Windows: switch it off, as on
    // Linux (off the async runtime: the radio can take a while to answer).
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        unsafe { CancelIoEx(self.device.0, null()) };
        if !self.keep.load(Ordering::Relaxed) {
            let addr = self.addr;
            std::thread::spawn(move || {
                let _ = disconnect(addr);
            });
        }
    }
}

impl Bluetooth {
    pub async fn open() -> Result<Self, Availability> {
        let found = spawn_blocking(|| radio().is_some()).await.unwrap_or(false);
        found.then_some(Bluetooth).ok_or(Availability::NoAdapter)
    }

    pub async fn powered(&self) -> bool {
        spawn_blocking(|| radio().is_some_and(|radio| unsafe { BluetoothIsConnectable(radio.0) } != 0)).await.unwrap_or(false)
    }

    pub async fn discover(&self) -> Result<Discovery, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                let Some(radio) = radio() else {
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                };
                forget_unusable(&radio);
                for info in wiimotes(&radio, INQUIRY) {
                    if info.fConnected == 0 && !stopped.load(Ordering::Relaxed) {
                        unsafe { BluetoothSetServiceState(radio.0, &info, &HID_SERVICE, BLUETOOTH_SERVICE_ENABLE) };
                    }
                }
            }
        });
        Ok(Discovery(stop))
    }

    pub async fn remotes(&self) -> Vec<Seen> {
        spawn_blocking(|| {
            let Some(radio) = radio() else { return Vec::new() };
            let hid = hid_devices();
            wiimotes(&radio, 0)
                .iter()
                .map(|info| Seen {
                    addr: format_addr(address(info)),
                    name: name(info),
                    in_range: info.fConnected != 0 && hid.iter().any(|(addr, _)| *addr == address(info)),
                    connected: false,
                })
                .collect()
        })
        .await
        .unwrap_or_default()
    }

    pub async fn connect(&self, addr: &str) -> Result<Link, String> {
        let addr = parse_addr(addr)?;
        spawn_blocking(move || {
            let (_, path) = hid_devices().into_iter().find(|(a, _)| *a == addr).ok_or_else(|| tr!("pas de réponse"))?;
            let device = unsafe {
                CreateFileW(
                    path.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    null_mut(),
                )
            };
            if device == INVALID_HANDLE_VALUE {
                return Err(last_error());
            }
            let device = Arc::new(Handle(device));
            // Opening works even for a remote that is gone; a write doesn't.
            write(&device, &[0x15, 0x00]).map_err(|_| tr!("pas de réponse"))?;
            let (tx, rx) = mpsc::unbounded_channel();
            let stop = Arc::new(AtomicBool::new(false));
            let (reader_device, reader_stop, reader_tx) = (device.clone(), stop.clone(), tx.clone());
            std::thread::spawn(move || read(reader_device, addr, reader_stop, reader_tx));
            Ok(Link { device, addr, reports: Mutex::new(rx), lost: tx, writing: Mutex::new(()), stop, keep: AtomicBool::new(false) })
        })
        .await
        .map_err(|e| e.to_string())?
    }

    /// Disconnects a remote from Windows (it then switches off).
    pub async fn release(&self, addr: &str) -> Result<(), String> {
        let addr = parse_addr(addr)?;
        spawn_blocking(move || disconnect(addr)).await.map_err(|e| e.to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses() {
        assert_eq!(format_addr(0x0017_AB3F_7C11), "00:17:AB:3F:7C:11");
        assert_eq!(parse_addr("00:17:AB:3F:7C:11"), Ok(0x0017_AB3F_7C11));
        assert!(parse_addr("00:17:AB").is_err());
    }

    /// Needs a Bluetooth adapter: lists the Wii Remotes Windows knows, without touching them.
    #[test]
    #[ignore]
    fn lists_known_remotes() {
        tauri::async_runtime::block_on(async {
            let Ok(bt) = Bluetooth::open().await else { panic!("no Bluetooth") };
            println!("powered: {}", bt.powered().await);
            for r in bt.remotes().await {
                println!("{} {} in_range={}", r.addr, r.name, r.in_range);
            }
            for (addr, path) in hid_devices() {
                println!("HID {} {}", format_addr(addr), string(&path));
            }
        });
    }

    /// Needs a remote in 1+2 mode within a minute: connects it, lights LED 1,
    /// rumbles, prints a few reports, then switches it off.
    #[test]
    #[ignore]
    fn connects_a_remote() {
        use super::super::protocol;
        tauri::async_runtime::block_on(async {
            let Ok(bt) = Bluetooth::open().await else { panic!("no Bluetooth") };
            let discovery = bt.discover().await.unwrap();
            let started = Instant::now();
            let seen = loop {
                if let Some(seen) = bt.remotes().await.into_iter().find(|r| r.in_range) {
                    break seen;
                }
                assert!(started.elapsed() < Duration::from_secs(60), "no remote");
                tokio::time::sleep(Duration::from_millis(500)).await;
            };
            drop(discovery);
            println!("found {} {} after {:?}", seen.addr, seen.name, started.elapsed());
            let link = bt.connect(&seen.addr).await.unwrap();
            for report in [protocol::leds(0), protocol::rumble(true)] {
                link.send(&report).await.unwrap();
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            link.send(&protocol::rumble(false)).await.unwrap();
            link.send(&protocol::status_request()).await.unwrap();
            link.send(&protocol::report_mode()).await.unwrap();
            let mut buf = [0u8; 32];
            for _ in 0..10 {
                let n = link.recv(&mut buf).await.unwrap();
                println!("{:02X?}", &buf[..n]);
            }
        });
    }
}
