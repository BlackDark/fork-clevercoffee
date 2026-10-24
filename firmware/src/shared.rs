//! State shared between tasks. One blocking mutex guards the machine and the config;
//! every holder keeps it for microseconds (no I/O under the lock).

use core::cell::RefCell;

use cc_api::Live;
use cc_core::history::History;
use cc_core::scale::ScaleReading;
use cc_core::{Config, Machine};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

/// Network facts the display, status and MQTT show.
#[cfg_attr(not(feature = "display"), allow(dead_code))]
#[derive(Debug, Clone, Copy, Default)]
pub struct NetState {
    pub wifi_connected: bool,
    pub ap_active: bool,
    pub rssi: i8,
    pub ip: [u8; 4],
    pub reconnects: u32,
    pub mqtt_connected: bool,
    pub ble_scale_connected: bool,
}

#[cfg_attr(not(feature = "display"), allow(dead_code))]
impl NetState {
    /// 0..=4 bars from the RSSI.
    pub fn signal(&self) -> u8 {
        match self.rssi {
            r if r >= -55 => 4,
            r if r >= -65 => 3,
            r if r >= -75 => 2,
            r if r >= -85 => 1,
            _ => 0,
        }
    }
}

pub struct Shared {
    pub cfg: Config,
    pub machine: Machine,
    /// 3.6 KB: on the heap.
    pub history: alloc::boxed::Box<History>,
    pub net: NetState,
    pub live: Live,
    /// Persist config / shot counter at the next storage pass.
    pub config_dirty: bool,
    pub shots_dirty: bool,
    /// A firmware image is being written: actuators stay off.
    pub ota_active: bool,
}

pub static SHARED: Mutex<CriticalSectionRawMutex, RefCell<Option<Shared>>> = Mutex::new(RefCell::new(None));

/// Runs `f` on the shared state. Panics before `init`.
pub fn with<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    SHARED.lock(|s| f(s.borrow_mut().as_mut().expect("shared state initialised")))
}

pub fn init(s: Shared) {
    SHARED.lock(|cell| *cell.borrow_mut() = Some(s));
}

/// Fresh sensor values, taken by the control loop each tick.
#[derive(Debug, Default, Clone, Copy)]
pub struct Pending {
    pub temperature: Option<f64>,
    pub temperature_fault: bool,
    pub scale: Option<ScaleReading>,
    pub pressure: Option<f32>,
}

pub static PENDING: Mutex<CriticalSectionRawMutex, RefCell<Pending>> = Mutex::new(RefCell::new(Pending {
    temperature: None,
    temperature_fault: false,
    scale: None,
    pressure: None,
}));

pub fn publish(f: impl FnOnce(&mut Pending)) {
    PENDING.lock(|p| f(&mut p.borrow_mut()));
}

pub fn take_pending() -> Pending {
    PENDING.lock(|p| {
        let mut p = p.borrow_mut();
        let out = *p;
        p.temperature = None;
        p.scale = None;
        p.pressure = None;
        out
    })
}

/// Wakes the storage task.
pub static PERSIST: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// Asks the scale task to tare (Bluetooth scales tare themselves).
pub static SCALE_TARE: Signal<CriticalSectionRawMutex, ()> = Signal::new();
/// Restart requested (after the HTTP response went out).
pub static RESTART: Signal<CriticalSectionRawMutex, RestartKind> = Signal::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartKind {
    Plain,
    FactoryReset,
    WifiReset,
}

/// Lowest free heap seen (sampled every 10 s by the main task).
pub static MIN_FREE_HEAP: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(u32::MAX);
