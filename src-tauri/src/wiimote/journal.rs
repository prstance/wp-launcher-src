//! The remotes' journal, in every build: <temp dir>/wp-wiimote.log (on Windows
//! %TEMP%\wp-wiimote.log), emptied when the launcher starts and capped. It tells what a
//! remote did on a machine we can't try ourselves: connections, failed writes, what it
//! answered, the MotionPlus found or not, and each report's pointer inputs (pointer.rs).

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

/// About half an hour of one remote's reports.
const CAP: u64 = 32 << 20;

struct Out {
    file: File,
    start: Instant,
    written: u64,
}

static OUT: Mutex<Option<Out>> = Mutex::new(None);

pub fn path() -> PathBuf {
    std::env::temp_dir().join("wp-wiimote.log")
}

/// One line, timed from the launcher's first one.
pub fn write(line: &str) {
    if cfg!(test) {
        return;
    }
    let Ok(mut out) = OUT.lock() else { return };
    if out.is_none() {
        let Ok(file) = File::create(path()) else { return };
        *out = Some(Out { file, start: Instant::now(), written: 0 });
    }
    let Some(o) = out.as_mut() else { return };
    if o.written >= CAP {
        return;
    }
    let line = format!("{:9.3} {line}\n", o.start.elapsed().as_secs_f32());
    o.written += line.len() as u64;
    if o.written >= CAP {
        let _ = o.file.write_all(b"# journal full\n");
    }
    let _ = o.file.write_all(line.as_bytes());
}

/// An event: in the journal and on stderr.
macro_rules! note {
    ($($arg:tt)*) => {{
        let line = format!($($arg)*);
        eprintln!("wiimote: {line}");
        $crate::wiimote::journal::write(&line);
    }};
}
pub(crate) use note;

/// A report's bytes, for the journal.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}
