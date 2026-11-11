//! The logger: every line goes to the serial port as before and into a RAM ring that
//! `GET /api/logs` serves, so a machine in its case can be debugged over WiFi.

use core::cell::RefCell;
use core::fmt::Write as _;

use cc_web::LogRing;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use log::{Level, LevelFilter, Log, Metadata, Record};

/// About a hundred lines: minutes of a quiet machine, seconds of a trace.
const RING_BYTES: usize = 6 * 1024;
/// A longer line is cut in the ring (the serial port gets all of it).
const LINE_BYTES: usize = 192;

static RING: Mutex<CriticalSectionRawMutex, RefCell<Option<LogRing>>> = Mutex::new(RefCell::new(None));

struct Tee;
static TEE: Tee = Tee;

impl Log for Tee {
    fn enabled(&self, _: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let (color, reset) = match record.level() {
            Level::Error => ("\u{1B}[31m", "\u{1B}[0m"),
            Level::Warn => ("\u{1B}[33m", "\u{1B}[0m"),
            Level::Info => ("\u{1B}[32m", "\u{1B}[0m"),
            Level::Debug => ("\u{1B}[34m", "\u{1B}[0m"),
            Level::Trace => ("\u{1B}[35m", "\u{1B}[0m"),
        };
        esp_println::println!("{color}{} - {}{reset}", record.level(), record.args());

        // Formatted before the lock: an argument may take locks of its own.
        if !RING.lock(|r| r.borrow().is_some()) {
            return;
        }
        let mut line: heapless::String<LINE_BYTES> = heapless::String::new();
        let _ = write!(
            line,
            "{:>8} {:<5} {}",
            embassy_time::Instant::now().as_millis(),
            record.level(),
            record.args()
        );
        crate::metrics::log_line(line.len() + 1);
        RING.lock(|r| {
            if let Some(ring) = r.borrow_mut().as_mut() {
                ring.push(line.as_bytes());
                ring.push(b"\n");
            }
        });
    }

    fn flush(&self) {}
}

/// Serial output only, until [`start_ring`].
pub fn init_logger(level: LevelFilter) {
    let _ = log::set_logger(&TEE);
    log::set_max_level(level);
}

/// Starts keeping lines for `/api/logs`. After the heap and the timer exist.
pub fn start_ring() {
    if let Some(ring) = LogRing::new(RING_BYTES) {
        RING.lock(|r| *r.borrow_mut() = Some(ring));
    }
}

/// The lines since offset `since` (see [`LogRing::read_since`]).
pub fn read_since(since: u64) -> Option<(u64, alloc::vec::Vec<u8>)> {
    // Reserved before the lock: the copy inside it must not allocate.
    let mut out = alloc::vec::Vec::new();
    out.try_reserve(RING_BYTES).ok()?;
    let next = RING.lock(|r| r.borrow().as_ref().map(|ring| ring.read_since(since, &mut out)))??;
    Some((next, out))
}
