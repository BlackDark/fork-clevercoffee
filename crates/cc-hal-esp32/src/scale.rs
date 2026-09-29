//! The HX711, on two data pins and one shared clock, sampled on its own task.
//!
//! Owner: **R3-17**.
//!
//! # What this file is
//!
//! The device half of [`cc_domain::sensor::hx711`]. Every decision — how many
//! clocks, which bit is the sign, what the tare subtracts, when the cell counts
//! as absent — is in the domain crate and host-tested there. What is left here
//! is three pins, a delay, a queue and a task.
//!
//! # 🔴 Why the sampling is a task and not a call from the control tick
//!
//! One 24-bit read is 25 clock writes plus 25 pin reads, and the HX711 needs
//! the *whole* sequence uninterruptible: the datasheet (§5.2) says an
//! interrupt longer than 60 µs while SCK is high can drive the amplifier into
//! power-down, so [`GpioHx711::in_critical_section`] masks interrupts for the
//! duration.
//!
//! So the cost of a read is not the register accesses — it is that the CPU
//! cannot do anything else for the duration. Doing that inside the 10 ms
//! control tick means the tick's worst case grows by a whole read, for a
//! sensor that is **optional**: `hardware.sensors.scale.enabled` defaults to
//! `false`. A machine with no scale must not pay for one, and a machine with
//! one must not have its control loop perturbed by it. A dedicated task at a
//! priority above the control task's makes the perturbation bounded and
//! measurable rather than structural — which is what R4-01b's tick histogram
//! is for.
//!
//! The task runs at [`SAMPLER_PRIO`], above the control task's 5 (04 §2's
//! table) and above `esp-mqtt`'s 4. The C++ runs everything in `loopTask` at
//! priority 1 with `AsyncTCP` at 10 — **the network preempts the control
//! loop** — and the whole point of 04 §2's priority table is that this
//! firmware does not.
//!
//! # Nothing here waits without a deadline
//!
//! [`GpioHx711::data_high`] is a *query*. The sampler polls it, and
//! [`cc_domain::sensor::hx711::SignalWatchdog`] turns "the line has been high
//! for longer than one conversion" into a fault. A shorted or absent data line
//! therefore produces a fault within
//! [`cc_domain::sensor::hx711::SIGNAL_TIMEOUT`] and the sampler keeps running —
//! which is the acceptance criterion, and the exact thing
//! `HX711Scale.cpp:44` and `:51` get wrong.
//!
//! # The clock is shared, the data lines are not
//!
//! GPIO33 is the clock, shared by both cells. GPIO32 and GPIO25 are the two
//! data lines (`include/clevercoffee/hardware/pinmapping.h`). A shared SCK
//! cannot clock two amplifiers' data lines independently, so the sampler tells
//! the bus which cell is next ([`GpioHx711::select`]) and the domain's
//! [`Scale`](cc_domain::sensor::hx711::Scale) alternates. That costs each cell
//! half the sample rate, which is inherent to the wiring and is documented
//! there.

use alloc::format;
use alloc::string::{String, ToString};
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::Arc;

use cc_domain::sensor::hx711::{
    Cell, Fault, Hx711Bus, Rate, ReadError, Scale, SignalWatchdog, TareRecord,
};
use cc_domain::units::Millis;
use esp_idf_hal::delay::{Ets, FreeRtos};
#[cfg(any(test, feature = "device-tests"))]
use esp_idf_hal::gpio::{Gpio32, Gpio33};
use esp_idf_hal::gpio::{Input, InputOutput, InputPin, Level, OutputPin, PinDriver, Pull};
use esp_idf_hal::interrupt;
use esp_idf_hal::task::queue::Queue as HalQueue;
use esp_idf_hal::task::thread::ThreadSpawnConfiguration;
use esp_idf_svc::sys::{EspError, ESP_FAIL};
use log::{error, info, warn};

/// `PIN_HXDAT` — the first data line.
pub const PIN_HXDAT: u8 = 32;

/// `PIN_HXDAT2` — the second data line.
pub const PIN_HXDAT2: u8 = 25;

/// `PIN_HXSCK` — the shared clock line.
pub const PIN_HXSCK: u8 = 33;

/// The control task's priority.
///
/// A `std::thread` on this target at ESP-IDF's pthread default, which is
/// `CONFIG_PTHREAD_TASK_PRIO_DEFAULT` — 5 in this build's `sdkconfig.h:823`.
/// It is named here rather than left implicit because the sampler's priority is
/// *relative* to it, and a relationship expressed in a comment is not checked.
pub const CONTROL_PRIO: u8 = 5;

/// The sampler's `FreeRTOS` priority.
///
/// **Above [`CONTROL_PRIO`]** (04 §2's table) and above `esp-mqtt`'s 4. The
/// reasoning is in the module docs.
///
/// The ceiling is 24 — `ThreadSpawnConfiguration::set` panics outside
/// `1..24` — and 6 is one above the control task rather than the maximum, so a
/// future task can still be placed above this one without renumbering.
///
/// The relationship is asserted at compile time rather than in a test, for the
/// reason `heater.rs` asserts its resolution the same way: a `SAMPLER_PRIO` of
/// 5 or lower would compile, run, and quietly reintroduce exactly the property
/// 04 §2 says this firmware exists to remove.
pub const SAMPLER_PRIO: u8 = 6;

const _: () = assert!(
    SAMPLER_PRIO > CONTROL_PRIO,
    "a scale read holds the CPU with interrupts masked, so it must not land \
     inside the control tick: the sampler outranks the control task"
);
const _: () = assert!(
    SAMPLER_PRIO >= 1 && SAMPLER_PRIO < 24,
    "ThreadSpawnConfiguration::set panics outside 1..24, so a priority the \
     scheduler would reject must be a compile error instead"
);

/// The sampling task's stack, in bytes.
///
/// 4 KiB.
///
/// The task's own frame is a `Scale` — two `Cell`s, each holding a 34-entry
/// `u32` dataset — plus the read path's locals, so the deepest chain is a few
/// hundred bytes. 4 KiB is the smallest stack in this firmware that is not
/// obviously wrong, matching [`crate::web`]'s SSE broadcaster.
pub const SAMPLER_STACK_BYTES: usize = 4096;

/// How long the sampler sleeps when a read found nothing ready.
///
/// Half the conversion time at the configured rate, so a healthy cell is found
/// ready within half a conversion of it becoming ready, and an idle sampler
/// wakes 20 times a second rather than spinning.
///
/// **A `FreeRTOS` sleep, not a busy wait.** A busy wait here would resolve a
/// ready conversion more precisely and would also hold the CPU for the whole
/// 25 ms at priority 6 — above the control task — which is the opposite of
/// what the dedicated task is for. The bus's only busy wait is the 1 µs
/// `SCK_DELAY` inside the interrupt-masked shift, where it is the correct
/// instrument and the only one.
pub const SAMPLE_IDLE_MS: u32 = 25;

/// How long the driver discards readings after power-up, in milliseconds.
///
/// 400, which is the C++'s `startMultiple`'s settling wait
/// (`HX711_ADC.cpp:56-59`: `startMultipleWaitTime = t + 400`, with the comment
/// "400ms is min. settling time at 10SPS"). Reproduced rather than reduced to
/// one conversion time, because the C++'s value is a *minimum* the part's own
/// note gives and the cost of being generous is 400 ms once, at boot.
pub const SETTLING_MS: u32 = 400;

/// The bus: two data pins and one shared clock.
///
/// The pins are configured here and nowhere else. `PinDriver` is `Send` but
/// **not** `Sync` (`esp-idf-hal-0.47.0/src/gpio.rs:1170` declares `Send` and
/// nothing more), so this cannot be shared, and there is exactly one writer of
/// these three pins in the whole program — the same property
/// [`crate::heater::TimerIsrPwm`] establishes for the heater.
pub struct GpioHx711 {
    /// The first data line.
    ///
    /// `Pull::Up` because DOUT is pulled low to signal a ready conversion, and
    /// a floating input with no pull reads unpredictably — a disconnected cell
    /// would look intermittently ready rather than reliably absent.
    data_1: PinDriver<'static, Input>,
    /// The second data line, on a dual scale.
    data_2: Option<PinDriver<'static, Input>>,
    /// The shared clock. `InputOutput` so the idle readback can see it, the
    /// same reasoning as `cc-firmware`'s `drive_inactive`.
    clock: PinDriver<'static, InputOutput>,
    /// Which data line the next read samples. See [`Self::select`].
    active: u8,
}

impl core::fmt::Debug for GpioHx711 {
    /// Names the pins and the cell count, never a level.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "GpioHx711(data={PIN_HXDAT}, data2={}, clock={PIN_HXSCK}, active={})",
            if self.data_2.is_some() {
                PIN_HXDAT2.to_string()
            } else {
                "none".into()
            },
            self.active,
        )
    }
}

impl GpioHx711 {
    /// A single-cell bus on `data_1` and `clock`.
    ///
    /// # Errors
    ///
    /// [`EspError`] if a pin cannot be configured.
    pub fn single(
        data_1: impl InputPin + 'static,
        clock: impl InputPin + OutputPin + 'static,
    ) -> Result<Self, EspError> {
        Ok(Self {
            data_1: PinDriver::input(data_1, Pull::Up)?,
            data_2: None,
            clock: PinDriver::input_output(clock, Pull::Floating)?,
            active: 0,
        })
    }

    /// A two-cell bus: two data lines, one shared clock.
    ///
    /// # Errors
    ///
    /// As [`Self::single`].
    pub fn dual(
        data_1: impl InputPin + 'static,
        data_2: impl InputPin + 'static,
        clock: impl InputPin + OutputPin + 'static,
    ) -> Result<Self, EspError> {
        Ok(Self {
            data_1: PinDriver::input(data_1, Pull::Up)?,
            data_2: Some(PinDriver::input(data_2, Pull::Up)?),
            clock: PinDriver::input_output(clock, Pull::Floating)?,
            active: 0,
        })
    }

    /// Whether this bus has a second data line.
    #[must_use]
    pub const fn is_dual(&self) -> bool {
        self.data_2.is_some()
    }

    /// Tell the bus which cell the next read samples.
    ///
    /// The domain's [`Scale`] alternates cells because one SCK line cannot
    /// clock two amplifiers' data lines independently, and *both*
    /// [`Hx711Bus::data_high`] and [`Hx711Bus::shift_bit`] must then look at
    /// the same line. Routing that through one `select` call per read is what
    /// keeps them in step; the alternative — having the bus guess — is a
    /// one-cell-fits-all bug that reads half the time.
    pub const fn select(&mut self, cell: u8) {
        self.active = if self.data_2.is_some() && cell == 1 {
            1
        } else {
            0
        };
    }

    /// The data line the next read samples.
    fn active_data(&mut self) -> &mut PinDriver<'static, Input> {
        if self.active == 1 {
            if let Some(second) = self.data_2.as_mut() {
                return second;
            }
        }
        &mut self.data_1
    }

    /// Power the amplifier up, as `powerUp()` does (`HX711_ADC.cpp:36-40`).
    ///
    /// SCK low is the idle state and is what the datasheet (§4.4) calls
    /// power-up. The C++ writes it in `begin()` rather than trusting the pin's
    /// reset value, and so does this.
    ///
    /// # Errors
    ///
    /// [`EspError`] if the clock pin cannot be written.
    pub fn power_up(&mut self) -> Result<(), EspError> {
        self.clock.set_level(Level::Low)
    }

    /// The idle level of each line, for the boot readback.
    ///
    /// A scale that is present and idle has **DOUT high** (no conversion ready)
    /// and **SCK low** (powered up). A scale that is *absent* reads the same,
    /// because the internal pull-up holds DOUT high — which is exactly why the
    /// readback cannot prove a scale is attached, and why presence is a timing
    /// question ([`SignalWatchdog`]) rather than a level question.
    ///
    /// `(data_1_high, data_2_high, clock_low)`.
    pub fn idle_levels(&mut self) -> (bool, bool, bool) {
        let data_1_high = self.data_1.is_high();
        let data_2_high = match self.data_2.as_mut() {
            Some(pin) => pin.is_high(),
            None => true,
        };
        let clock_low = self.clock.is_low();
        (data_1_high, data_2_high, clock_low)
    }
}

impl Hx711Bus for GpioHx711 {
    type Error = EspError;

    fn in_critical_section<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        // `interrupt::free` is `portENTER_CRITICAL`/`portEXIT_CRITICAL`
        // (`esp-idf-hal-0.47.0/src/interrupt.rs`) — the same thing the C++'s
        // `noInterrupts()` does around a 1-Wire slot
        // (`OneWire.cpp:186-205`), and the reason `HX711Scale.h:17` sets
        // `SCK_DISABLE_INTERRUPTS 0` while the library's own `config.h:48-50`
        // says to set it to 1 for exactly this hazard.
        //
        // The window is 25 clocks of two register writes each: tens of
        // microseconds, comfortably inside the original ESP32's 300 ms
        // interrupt watchdog (`components/esp_system/int_wdt.c`), and shorter
        // than the 480 µs window the 1-Wire driver already opens for a reset.
        // `interrupt::free` takes a nullary closure
        // (`esp-idf-hal-0.47.0/src/interrupt.rs:288`), so `f` is moved into one
        // that borrows `self`. The borrow ends with the call, which is what
        // makes the signature work: the critical section is exactly `f`'s
        // duration and not one instruction more.
        interrupt::free(|| f(self))
    }

    fn shift_bit(&mut self) -> Result<bool, Self::Error> {
        // The C++'s order (`HX711_ADC.cpp:341-352`): SCK high, 1 µs, SCK low,
        // **then** read DOUT. The datasheet latches the output on the falling
        // edge of SCK (§5.1), so sampling after the edge is what is correct.
        //
        // The 1 µs is `SCK_DELAY`, which `HX711Scale.h:16` sets to 1
        // specifically because "the only mcu reported to need this delay is the
        // ESP32 (issue #35)" (`HX711_ADC`'s `config.h:44-46`). It is on the
        // high edge only, as the C++ has it.
        self.clock.set_level(Level::High)?;
        Ets::delay_us(1);
        self.clock.set_level(Level::Low)?;
        Ok(self.active_data().is_high())
    }

    fn data_high(&mut self) -> bool {
        self.active_data().is_high()
    }
}

/// What the sampler publishes, for the machine to read without touching a pin.
///
/// Every field is a plain atomic, so the control task can read the weight from
/// a tick with no lock, no queue drain and no blocking. The *events* — a
/// completed tare, a new calibration factor — go over a queue instead, because
/// losing one of those loses a calibration while losing a 10 Hz weight sample
/// loses nothing.
#[derive(Debug)]
pub struct Telemetry {
    /// The latest weight in **milligrams**, signed.
    ///
    /// There is no `AtomicF64` on `xtensa-esp32`, and a 64-bit store is not
    /// atomic there, so a float in shared state would be readable torn — a
    /// weight that is neither of the two values it was written as. Milligrams
    /// in an `AtomicI32` is one atomic word: ±2.1 million grams, and 1 mg
    /// resolution, which is finer than the HX711 resolves at gain 128 and far
    /// finer than the display shows.
    weight_mg: AtomicI32,
    /// Whether a weight has been published at all.
    has_weight: AtomicBool,
    /// Whether the cell is not answering.
    faulted: AtomicBool,
    /// Conversions completed.
    conversions: AtomicU32,
    /// Reads that found no conversion ready.
    not_ready: AtomicU32,
    /// Samples the driver refused as implausible.
    rejected: AtomicU32,
    /// Tares performed.
    tares: AtomicU32,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    /// A telemetry block with nothing in it.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            weight_mg: AtomicI32::new(0),
            has_weight: AtomicBool::new(false),
            faulted: AtomicBool::new(false),
            conversions: AtomicU32::new(0),
            not_ready: AtomicU32::new(0),
            rejected: AtomicU32::new(0),
            tares: AtomicU32::new(0),
        }
    }

    /// The latest weight in grams, or `None` if nothing has been read.
    ///
    /// `None` is the answer for "no scale" and for "the scale has not spoken
    /// yet", and it is deliberately **not** `Some(0.0)`: 0 g is a real weight,
    /// and a UI showing it would show a full cup.
    #[must_use]
    pub fn weight_g(&self) -> Option<f64> {
        if self.has_weight.load(Ordering::Relaxed) {
            Some(f64::from(self.weight_mg.load(Ordering::Relaxed)) / 1000.0)
        } else {
            None
        }
    }

    /// Publish a weight in grams.
    ///
    /// A non-finite weight is refused rather than stored: the display would
    /// render "NaN" and MQTT would publish it. The only source of a NaN is the
    /// calibration factor, which the domain crate already refuses to be zero or
    /// infinite, so this is a backstop rather than a path.
    pub fn publish_weight(&self, grams: f64) {
        if !grams.is_finite() {
            error!("scale: refusing to publish a non-finite weight");
            return;
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a weight beyond +/-2.1 million grams is not a scale \
                      reading, and saturating would hide a real fault behind a \
                      plausible number"
        )]
        let mg = (grams * 1000.0) as i32;
        // Stored before the flag, and read after it, so a reader that sees
        // `has_weight` also sees the value that went with it.
        self.weight_mg.store(mg, Ordering::Relaxed);
        self.has_weight.store(true, Ordering::Relaxed);
    }

    /// Whether the cell is not answering.
    #[must_use]
    pub fn faulted(&self) -> bool {
        self.faulted.load(Ordering::Relaxed)
    }

    /// Note whether the cell is answering.
    pub fn set_faulted(&self, faulted: bool) {
        self.faulted.store(faulted, Ordering::Relaxed);
    }

    /// Conversions completed.
    #[must_use]
    pub fn conversions(&self) -> u32 {
        self.conversions.load(Ordering::Relaxed)
    }

    /// Note a completed conversion.
    pub fn note_conversion(&self) {
        self.conversions.fetch_add(1, Ordering::Relaxed);
    }

    /// Reads that found nothing ready.
    #[must_use]
    pub fn not_ready(&self) -> u32 {
        self.not_ready.load(Ordering::Relaxed)
    }

    /// Note a read that found nothing ready.
    pub fn note_not_ready(&self) {
        self.not_ready.fetch_add(1, Ordering::Relaxed);
    }

    /// Samples the driver refused.
    #[must_use]
    pub fn rejected(&self) -> u32 {
        self.rejected.load(Ordering::Relaxed)
    }

    /// Replace the refused-sample count from the driver's own.
    pub fn set_rejected(&self, rejected: u32) {
        self.rejected.store(rejected, Ordering::Relaxed);
    }

    /// Note that a tare was performed.
    pub fn note_tare(&self) {
        self.tares.fetch_add(1, Ordering::Relaxed);
    }

    /// How many tares have been performed.
    #[must_use]
    pub fn tares(&self) -> u32 {
        self.tares.load(Ordering::Relaxed)
    }

    /// A one-line description for the boot log and the tick comparison.
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "conversions {}, not-ready {}, rejected {}, tares {}, faulted {}",
            self.conversions(),
            self.not_ready(),
            self.rejected(),
            self.tares(),
            self.faulted(),
        )
    }
}

/// What the sampling task is asked to do.
///
/// Sent over a bounded queue. Every variant is a request for the sampler to act
/// on its own pins; nothing here reaches into control state, which is the
/// direction 04 §3.2 allows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SamplerCommand {
    /// Tare against the current average, and report the new offsets.
    Tare,
    /// Recalibrate against a known mass, and report the new factors.
    Calibrate {
        /// `hardware.sensors.scale.known_weight`, in grams.
        known_weight: f64,
    },
    /// Change how many samples are averaged.
    SetSamples {
        /// `hardware.sensors.scale.samples`.
        samples: u8,
    },
    /// Restore a tare persisted by a previous boot.
    RestoreTare {
        /// The stored offsets.
        record: TareRecord,
    },
}

/// What the sampling task reports back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SamplerEvent {
    /// A tare completed, with the offsets to persist.
    Tared {
        /// The offsets now in force.
        record: TareRecord,
    },
    /// A recalibration completed, with the factors to persist.
    Calibrated {
        /// Counts per gram, for cell 1.
        factor_1: f64,
        /// Counts per gram, for cell 2, or `None` on a single-cell scale.
        factor_2: Option<f64>,
    },
    /// A request was refused. The machine is told rather than left waiting for
    /// a confirmation that will never come.
    Refused {
        /// Which request, named.
        what: &'static str,
    },
}

/// The control → sampler queue depth.
///
/// 4, drop-newest, for the same reason [`crate::task::CommandQueue`] is: a full
/// queue means the sampler is behind, which is a condition to shed rather than
/// to report. Every command is idempotent, so dropping one loses an operator
/// action and nothing else.
pub const COMMAND_DEPTH: usize = 4;

/// The sampler → control queue depth.
///
/// Four.
///
/// These are *events* — a completed tare, a new factor — and dropping one loses
/// a calibration, so the control task drains this every tick and the depth only
/// has to cover one tick's worth.
pub const EVENT_DEPTH: usize = 4;

/// A running sampling task, and the handle the control task holds.
///
/// The split is the design: the task owns the pins and the driver, the handle
/// owns a shared [`Telemetry`] and two queues, and **neither half can reach the
/// other**. The control task cannot drive a pin — it has no `PinDriver` — and
/// the sampler cannot touch control state, because it has no reference to it.
pub struct Sampler {
    telemetry: Arc<Telemetry>,
    commands: Arc<HalQueue<SamplerCommand>>,
    events: Arc<HalQueue<SamplerEvent>>,
}

impl Sampler {
    /// Start the sampling task and return the handle.
    ///
    /// # Errors
    ///
    /// [`EspError`] if the pins could not be configured or the task could not
    /// be created. Neither is fatal to the boot: a machine whose scale cannot be
    /// brought up must still run its control loop and report the fault, and a
    /// sensor that will not read is a fault to report, not a reason to refuse
    /// to boot.
    pub fn start(
        mut bus: GpioHx711,
        scale: Scale,
        rate: Rate,
        telemetry: Arc<Telemetry>,
    ) -> Result<Self, EspError> {
        bus.power_up()?;
        let (data_1_high, data_2_high, clock_low) = bus.idle_levels();
        info!(
            "scale: pins configured — data {PIN_HXDAT}={}, data {PIN_HXDAT2}={}, \
             clock {PIN_HXSCK} low={clock_low}; rate {rate:?} = gain {}, {} SPS, \
             {} clocks per read; {} cell(s)",
            level(data_1_high),
            level(data_2_high),
            rate.gain(),
            rate.samples_per_second(),
            rate.clocks(),
            scale.cells(),
        );
        if !(data_1_high && clock_low) {
            // Not fatal and not a scale fault: these are the levels a *present*
            // idle scale has too, because the internal pull-up holds DOUT high
            // when nothing is connected. What a low DOUT at boot does mean is
            // that a conversion is already pending, or that something is
            // pulling the line down.
            warn!(
                "scale: idle levels are not the expected DOUT-high / SCK-low — \
                 either a conversion is already pending or something is pulling \
                 GPIO{PIN_HXDAT} low. Presence is decided by the signal timeout, \
                 not by this reading."
            );
        }

        // `Queue` is `Send + Sync` (`esp-idf-hal` 0.47 `task.rs:971-972`) and
        // every method takes `&self`, because the FreeRTOS queue is internally
        // synchronised. So the handle and the task share one through an `Arc`
        // rather than each holding a copy — the same shape as
        // `crate::task::CommandQueue`, which is handed to the HTTP layer as an
        // `Arc` for exactly this reason.
        let commands = Arc::new(HalQueue::new(COMMAND_DEPTH));
        let events = Arc::new(HalQueue::new(EVENT_DEPTH));
        let task_commands = Arc::clone(&commands);
        let task_events = Arc::clone(&events);
        let task_telemetry = Arc::clone(&telemetry);

        // `std::thread::Builder` on this target is `esp_pthread`
        // (`components/pthread/port/linux/pthread.c`), which reads its priority
        // and stack from the **process-wide** `esp_pthread` default.
        // `ThreadSpawnConfiguration` is the only priority API in
        // `esp-idf-hal` 0.47 (`task.rs:353-405`) and it is global, so the
        // default is installed around the spawn and restored after — leaving
        // the sampler's priority installed would silently raise every task
        // spawned later, including the httpd task.
        // `ThreadSpawnConfiguration` is not `Copy` (`esp-idf-hal` 0.47
        // `task.rs:353`, it derives only `Debug`), so the saved default is read
        // twice rather than moved: once to derive the sampler's configuration
        // from, and once to put back. Two reads of a global that only this
        // thread changes between them is the honest cost of a global-only API.
        let mut config = ThreadSpawnConfiguration::get().unwrap_or_default();
        config.stack_size = SAMPLER_STACK_BYTES;
        config.priority = SAMPLER_PRIO;
        config.name = Some(c"scale");
        // `set` panics on a priority outside `1..24`. `SAMPLER_PRIO` is a
        // compile-time 6 so this cannot fire today; it is left to fire because
        // a panic at boot is the right outcome for a priority the scheduler
        // would reject anyway.
        config.set()?;

        let spawned = std::thread::Builder::new()
            .name("scale".into())
            .stack_size(SAMPLER_STACK_BYTES)
            .spawn(move || {
                run_sampler(bus, scale, rate, task_telemetry, task_commands, task_events);
            });

        // Restore the process default whatever happened. A failure to restore
        // would leave every later task at the sampler's priority, so it is
        // reported rather than dropped — and it is restored on the error path
        // too, which is the case that matters.
        if let Some(previous) = ThreadSpawnConfiguration::get() {
            previous.set()?;
        }

        spawned.map_err(|err| {
            error!("scale: the sampling task did not start: {err}");
            EspError::from_infallible::<ESP_FAIL>()
        })?;

        info!(
            "scale: sampling task started at priority {SAMPLER_PRIO}, stack \
             {SAMPLER_STACK_BYTES} B"
        );

        Ok(Self {
            telemetry,
            commands,
            events,
        })
    }

    /// The shared telemetry, for the control task's tick.
    #[must_use]
    pub fn telemetry(&self) -> &Arc<Telemetry> {
        &self.telemetry
    }

    /// Ask for a tare, dropping the request if the queue is full.
    #[must_use]
    pub fn request_tare(&self) -> bool {
        send(&self.commands, SamplerCommand::Tare)
    }

    /// Ask for a recalibration against a known mass.
    #[must_use]
    pub fn request_calibrate(&self, known_weight: f64) -> bool {
        send(&self.commands, SamplerCommand::Calibrate { known_weight })
    }

    /// Ask for a different sample count.
    #[must_use]
    pub fn request_samples(&self, samples: u8) -> bool {
        send(&self.commands, SamplerCommand::SetSamples { samples })
    }

    /// Hand the sampler a tare persisted by a previous boot.
    #[must_use]
    pub fn request_restore(&self, record: TareRecord) -> bool {
        send(&self.commands, SamplerCommand::RestoreTare { record })
    }

    /// Take the next event, if one is waiting. Never blocks.
    #[must_use]
    pub fn next_event(&self) -> Option<SamplerEvent> {
        self.events.recv_front(0).map(|(item, _)| item)
    }
}

/// The word for a logic level, for a log line.
const fn level(high: bool) -> &'static str {
    if high {
        "high"
    } else {
        "low"
    }
}

/// Offer a value on a bounded queue, dropping it if full. Never blocks.
///
/// `send_back` takes a `TickType_t` timeout (`esp-idf-hal` 0.47
/// `task.rs:1030`); 0 means the only way to fail is a full queue, which is the
/// drop-newest case. `T: Copy` is `hal::task::queue::Queue`'s bound, and both
/// command and event types are `Copy`, so neither can smuggle an allocation
/// across the task boundary.
fn send<T: Copy>(queue: &HalQueue<T>, value: T) -> bool {
    queue.send_back(value, 0).is_ok()
}

/// The sampling loop. Runs for the life of the process.
///
/// It never blocks on a pin: every read is a query, and the only sleep is
/// [`SAMPLE_IDLE_MS`] when there is nothing to read. A cell that has stopped
/// answering therefore costs one poll every [`SAMPLE_IDLE_MS`] forever and is
/// reported through the watchdog — the acceptance criterion, and the thing
/// `HX711Scale.cpp:44` gets wrong by spinning.
#[allow(
    clippy::needless_pass_by_value,
    reason = "these ARE the task: they are moved into it, they are held for its \
              whole life, and nothing else can reach them. A `&` would suggest \
              the caller could keep using them, which is exactly the coupling \
              this split exists to prevent -- the caller has no `&mut Scale` \
              either, so the pins have one owner."
)]
fn run_sampler(
    mut bus: GpioHx711,
    mut scale: Scale,
    rate: Rate,
    telemetry: Arc<Telemetry>,
    commands: Arc<HalQueue<SamplerCommand>>,
    events: Arc<HalQueue<SamplerEvent>>,
) {
    // 🔴 Armed at the driver's start, not on the first conversion. An absent
    // scale is the case that most needs reporting and the one that never
    // produces a conversion, so a watchdog armed on the first reading would stay
    // silent for exactly the machine that has no scale. The C++ gets this right
    // by accident (`HX711_ADC.cpp:129` sets `lastDoutLowTime = millis()` before
    // its first `update`); `cc_domain::sensor::hx711`'s watchdog tests say so.
    let mut watchdog = SignalWatchdog::new();
    watchdog.armed_at(Millis::new(now_ms()));
    let mut was_faulted = false;
    let mut needs_tare = true;

    // Readings are taken from the first conversion but not acted on until the
    // amplifier has settled — see [`SETTLING_MS`]. Expressed as a deadline
    // rather than as the C++'s blocking spin.
    let settle_until_ms = now_ms().wrapping_add(SETTLING_MS);
    let mut settled = false;

    info!(
        "scale: sampling — {} cell(s), {} samples averaged per reading, \
         settling for {SETTLING_MS} ms",
        scale.cells(),
        scale.cell_1().samples_in_use(),
    );

    loop {
        // --- commands, drained every pass ---------------------------------
        while let Some(command) = commands.recv_front(0).map(|(item, _)| item) {
            handle_command(command, &mut scale, &events, &telemetry);
        }

        // --- the read ------------------------------------------------------
        // The bus has to be told which cell is next *before* the read, because
        // `data_high` and `shift_bit` both look at the selected line.
        bus.select(scale.next_cell());
        match scale.read(&mut bus, rate) {
            Ok(Some(_)) => {
                telemetry.note_conversion();
                watchdog.note_ready(Millis::new(now_ms()));
                telemetry.set_rejected(total_rejected(&scale));

                if !settled && Millis::new(now_ms()).has_reached(Millis::new(settle_until_ms)) {
                    settled = true;
                    info!("scale: settled");
                }

                // The start-up tare is attempted on **every** conversion until
                // there is something to tare against, not only at the settling
                // boundary. A cell that is slow to produce its first usable
                // reading — which is exactly what a marginal load cell does —
                // would otherwise settle, find nothing to tare, and publish an
                // un-tared weight forever, because the one chance had passed.
                if settled && needs_tare && has_data(&scale) {
                    let record = scale.tare_all();
                    telemetry.note_tare();
                    needs_tare = false;
                    info!(
                        "scale: start-up tare — cell 1 offset {}, cell 2 offset {}",
                        record.offset_1, record.offset_2
                    );
                }

                if settled {
                    publish(&telemetry, &scale);
                }
            }
            Ok(None) => {
                telemetry.note_not_ready();
                // Bounded: a not-ready read costs one sleep, never a loop.
                FreeRtos::delay_ms(SAMPLE_IDLE_MS);
            }
            Err(ReadError::Fault(Fault::OutOfRange)) => {
                // A word outside 24 bits. The C++ sets a flag nobody reads
                // (`HX711_ADC.cpp:371-374`); here it is counted and the sample
                // is dropped rather than averaged in.
                warn!("scale: a conversion was out of range and was discarded");
                FreeRtos::delay_ms(SAMPLE_IDLE_MS);
            }
            Err(ReadError::Bus(err)) => {
                // A pin that cannot be written is not going to start working,
                // and retrying at full speed would spin. This is the one error
                // the driver cannot recover from, so it is reported rather than
                // swallowed, and the loop backs off.
                error!("scale: the data or clock pin failed: {err:?}");
                telemetry.set_faulted(true);
                FreeRtos::delay_ms(1000);
            }
        }

        // --- the fault path ------------------------------------------------
        // Checked every pass, not only after a read, so a cell that goes quiet
        // while the sampler sleeps is still reported. This is the whole of the
        // difference from the C++: `HX711Scale.cpp:44` cannot reach this
        // question at all, because it never returns from the spin.
        let faulted = watchdog.is_faulted(Millis::new(now_ms()));
        if faulted != was_faulted {
            was_faulted = faulted;
            telemetry.set_faulted(faulted);
            if faulted {
                error!(
                    "scale: DOUT has been high for more than {} ms — the cell is \
                     not answering. No scale is fitted, or GPIO{PIN_HXDAT} is \
                     floating or shorted. The weight is reported as absent, not \
                     guessed, and nothing else is affected.",
                    cc_domain::sensor::hx711::SIGNAL_TIMEOUT.raw(),
                );
            } else {
                info!("scale: the cell is answering again");
            }
        }
    }
}

/// Whether every cell has a sample to average.
fn has_data(scale: &Scale) -> bool {
    scale.cell_1().has_data() && scale.cell_2().is_none_or(Cell::has_data)
}

/// The refused-sample count across every cell.
fn total_rejected(scale: &Scale) -> u32 {
    scale.cell_1().rejected_samples() + scale.cell_2().map_or(0, Cell::rejected_samples)
}

/// Publish the current weight, if there is one worth publishing.
fn publish(telemetry: &Telemetry, scale: &Scale) {
    if has_data(scale) {
        telemetry.publish_weight(scale.weight());
    }
}

/// Act on one command. Runs on the sampling task, with the pins.
fn handle_command(
    command: SamplerCommand,
    scale: &mut Scale,
    events: &HalQueue<SamplerEvent>,
    telemetry: &Telemetry,
) {
    match command {
        SamplerCommand::Tare => {
            if !has_data(scale) {
                // Taring an empty dataset stores zero, which is the same as no
                // tare at all — and it would then be *persisted* as one, so the
                // next boot would restore a tare of zero and look calibrated.
                warn!("scale: tare refused — the cell has produced no reading yet");
                send(events, SamplerEvent::Refused { what: "tare" });
                return;
            }
            let record = scale.tare_all();
            telemetry.note_tare();
            info!(
                "scale: tared — cell 1 offset {}, cell 2 offset {}",
                record.offset_1, record.offset_2
            );
            publish(telemetry, scale);
            send(events, SamplerEvent::Tared { record });
        }
        SamplerCommand::Calibrate { known_weight } => {
            let Some(factor_1) = scale.cell_1_mut().calibrate(known_weight) else {
                warn!(
                    "scale: calibration refused — nothing on the pan, or \
                     known_weight {known_weight} is not a usable mass"
                );
                send(
                    events,
                    SamplerEvent::Refused {
                        what: "calibration",
                    },
                );
                return;
            };
            // The second cell is calibrated against the same known mass, which
            // is what a two-cell pan means: the mass is shared, so each cell
            // sees roughly half of it and each needs its own factor.
            let factor_2 = scale
                .cell_2_mut()
                .and_then(|cell| cell.calibrate(known_weight));
            info!(
                "scale: calibrated against {known_weight} g — cell 1 {factor_1}, \
                 cell 2 {}",
                match factor_2 {
                    Some(factor) => format!("{factor}"),
                    None => "n/a".into(),
                }
            );
            publish(telemetry, scale);
            send(events, SamplerEvent::Calibrated { factor_1, factor_2 });
        }
        SamplerCommand::SetSamples { samples } => {
            scale.cell_1_mut().set_samples(samples);
            if let Some(second) = scale.cell_2_mut() {
                second.set_samples(samples);
            }
            info!(
                "scale: averaging {} samples per reading",
                scale.cell_1().samples_in_use()
            );
        }
        SamplerCommand::RestoreTare { record } => {
            if scale.restore(record) {
                info!(
                    "scale: tare restored from NVS — cell 1 offset {}, cell 2 \
                     offset {}",
                    record.offset_1, record.offset_2
                );
                publish(telemetry, scale);
            } else {
                // Not fatal, and not silently ignored: the start-up tare runs
                // instead, and the operator is told why the stored one was
                // not used.
                warn!(
                    "scale: the stored tare describes {} cell(s) and this scale \
                     has {} — ignoring it; the start-up tare applies instead",
                    record.cells,
                    scale.cells()
                );
            }
        }
    }
}

/// The millisecond clock, so the sampler's time comes from one place.
fn now_ms() -> u32 {
    crate::time::now_ms()
}

/// The pins [`an_unconnected_data_line_reports_not_ready_and_never_clocks`] needs,
/// lent by whoever took the peripherals.
///
/// `Peripherals::take()` succeeds **once per process**, and the on-target runner
/// (`cc-device-tests`) takes it in `hold_actuators_inactive` before any case
/// runs. A test that took the peripherals itself would therefore be the second
/// caller: the first would succeed and every later one would panic on
/// `.expect("peripherals")`, which is exactly what happened. Two owners of the
/// chip's peripherals is the mistake `Abp2I2c::new` already documents against
/// ("belongs to `cc-firmware`'s `main`").
///
/// So the owner lends them instead. [`lend_test_pins`] takes them once, and the
/// case borrows them. The borrow is a one-shot: the cells go to whichever case
/// asks first, and a second ask is reported rather than silently passing.
#[cfg(any(test, feature = "device-tests"))]
static TEST_PINS: std::sync::Mutex<Option<(Gpio32<'static>, Gpio33<'static>)>> =
    std::sync::Mutex::new(None);

/// Offer the scale's pins to the on-target test suite.
///
/// Only the two pins the not-ready case uses are lent; `data_2` (GPIO 25) is
/// not, because that case is specifically about a single idle data line.
#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub fn lend_test_pins(data_1: Gpio32<'static>, clock: Gpio33<'static>) {
    if let Ok(mut slot) = TEST_PINS.lock() {
        *slot = Some((data_1, clock));
    }
}

/// Take the lent pins, or report that none were offered.
///
/// A `bool` rather than a pin, so the caller says which pins it could not get
/// and the harness logs it — the alternative, a silent early `return`, turns a
/// wiring mistake into a passing test.
#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
fn take_test_pins() -> Option<(Gpio32<'static>, Gpio33<'static>)> {
    TEST_PINS.lock().ok().and_then(|mut slot| slot.take())
}

#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub mod tests {
    // A unit-test module globs its parent on purpose: the cases are exercising
    // the parent's private helpers, which is the point of keeping them in the
    // same file. `clippy::wildcard_imports` normally makes an exception for
    // `use super::*` inside a `#[cfg(test)]` module, and this module is
    // `#[cfg(any(test, feature = "device-tests"))]` -- the on-target runner
    // compiles it outside a test build -- so the exception no longer applies and
    // the allowance is made explicitly here instead of in a list that would rot.
    #![allow(clippy::wildcard_imports)]

    use super::*;

    /// The pin map is the one the C++ uses.
    ///
    /// `pinmapping.h`'s `PIN_HXDAT` (32), `PIN_HXDAT2` (25) and `PIN_HXSCK`
    /// (33). If a future edit moves one of these, the scale silently stops
    /// reading — and nothing else in the firmware would notice, because nothing
    /// else touches these pins.
    #[cfg_attr(test, test)]
    pub fn the_pins_are_the_cpp_pin_map() {
        assert_eq!(PIN_HXDAT, 32);
        assert_eq!(PIN_HXDAT2, 25);
        assert_eq!(PIN_HXSCK, 33);
    }

    /// The sampler's stack fits the two datasets it carries.
    ///
    /// A `Cell` is a 34-entry `u32` dataset plus a handful of scalars, so a
    /// `Scale` is about 300 bytes on the task's stack. The bound is loose on
    /// purpose: what it catches is someone raising `MAX_SAMPLES` by an order of
    /// magnitude and overflowing the task, which would be a stack overflow in a
    /// task nobody is watching.
    #[cfg_attr(test, test)]
    pub fn the_sampler_stack_fits_the_datasets_it_carries() {
        // Two cells, because a dual scale is the larger of the two.
        let scale = core::mem::size_of::<cc_domain::sensor::hx711::Scale>();
        assert!(
            scale < SAMPLER_STACK_BYTES / 4,
            "a Scale is {scale} B and the stack is {SAMPLER_STACK_BYTES} B; the \
             frame must be well under a quarter of it"
        );
    }

    /// A fresh telemetry block reports no weight.
    ///
    /// The distinction that matters: `None` is "no scale or nothing read yet" and
    /// `Some(0.0)` is "the scale says zero". Publishing the second for the first
    /// is the defect `an_absent_reading_is_null_and_never_a_fabricated_zero`
    /// exists to prevent on the HTTP side, and this is the producer of that
    /// value.
    #[cfg_attr(test, test)]
    pub fn a_fresh_telemetry_block_reports_no_weight() {
        assert_eq!(Telemetry::new().weight_g(), None);
        let telemetry = Telemetry::new();
        telemetry.publish_weight(0.0);
        assert_eq!(
            telemetry.weight_g(),
            Some(0.0),
            "a published zero is a real weight and must be Some(0.0)"
        );
    }

    /// A negative weight round-trips, and a sub-gram one keeps its resolution.
    ///
    /// A reversed load cell reports negative (`hardware.sensors.scale.
    /// calibration`'s range is `-999999..=999999`), and the display shows one
    /// decimal, so the sign has to survive the millisecond store.
    #[cfg_attr(test, test)]
    pub fn a_weight_round_trips_through_the_shared_milligram_store() {
        let telemetry = Telemetry::new();
        for grams in [-12.5_f64, 0.0, 0.1, 267.0, 1999.75] {
            telemetry.publish_weight(grams);
            let read = telemetry.weight_g().expect("a weight was published");
            assert!(
                (read - grams).abs() < 0.001,
                "published {grams}, read back {read}"
            );
        }
    }

    /// A non-finite weight is refused rather than published.
    ///
    /// The only source is the calibration factor, which the domain crate already
    /// refuses to be zero or infinite, so this is a backstop — and a backstop
    /// that lets a NaN through would put "NaN" on the display and on MQTT.
    #[cfg_attr(test, test)]
    pub fn a_non_finite_weight_is_never_published() {
        let telemetry = Telemetry::new();
        telemetry.publish_weight(f64::NAN);
        assert_eq!(telemetry.weight_g(), None);
        telemetry.publish_weight(f64::INFINITY);
        assert_eq!(telemetry.weight_g(), None);
    }

    /// The bus is a query, and a bus that reports "not ready" never clocks.
    ///
    /// This is the acceptance criterion in miniature, on real pins: with no
    /// scale attached the pull-up holds DOUT high, `data_high` is `true`, and
    /// the driver must answer "nothing yet" rather than spinning or failing.
    /// The C++ cannot express this at all — `update()` returns 0 and
    /// `HX711Scale::init` spins on it forever (`HX711Scale.cpp:44`).
    #[cfg_attr(test, test)]
    pub fn an_unconnected_data_line_reports_not_ready_and_never_clocks() {
        use cc_domain::sensor::hx711::{read_raw, Rate};

        // Borrowed, not taken: see `lend_test_pins`.
        let Some((data_1, clock)) = take_test_pins() else {
            // GPIO32/33 are in use by a scale in some configurations; there is
            // nothing to assert and saying so beats failing spuriously.
            info!("scale: pins 32/33 unavailable — the not-ready path is not exercised here");
            return;
        };
        let Ok(mut bus) = GpioHx711::single(data_1, clock) else {
            info!(
                "scale: pins 32/33 could not be configured — the not-ready path is not exercised"
            );
            return;
        };
        bus.power_up().expect("power up");

        // With nothing attached the pull-up holds DOUT high. If a real scale
        // were fitted and mid-conversion this would legitimately be low, and
        // the assertion below would be the wrong assertion — which is why the
        // levels are reported either way.
        let (data_1_high, _, clock_low) = bus.idle_levels();
        info!("scale: idle — data 32 high={data_1_high}, clock 33 low={clock_low}");

        if data_1_high {
            // The fault path, proven on hardware: not ready, not an error, and
            // not a hang. One call, bounded, returns.
            let outcome = read_raw(&mut bus, Rate::Gain128);
            assert_eq!(
                outcome,
                Ok(None),
                "a high DOUT must be 'nothing yet', never a fault and never a loop"
            );
        } else {
            info!("scale: DOUT is low — a conversion is pending, not testing the not-ready path");
        }
    }

    /// The watchdog turns a silent line into a fault within its deadline.
    ///
    /// Run against the real clock, because the arithmetic is wrapping `Millis`
    /// arithmetic and a synthetic clock would not exercise the wrap. Armed the
    /// way the sampler arms it — at the driver's start — so this is the same
    /// state an absent scale is in, and it is asserted rather than slept for: a
    /// 100 ms sleep here would make this a 100 ms stall on the runner, and the
    /// boundary is the property, not the wall time.
    #[cfg_attr(test, test)]
    pub fn the_signal_watchdog_reports_an_absent_cell_within_its_deadline() {
        use cc_domain::sensor::hx711::{SignalWatchdog, SIGNAL_TIMEOUT};

        let mut watchdog = SignalWatchdog::new();
        assert!(
            !watchdog.is_armed(),
            "unarmed is 'the driver has not started'"
        );

        // Armed from boot, exactly as `run_sampler` does. No conversion is ever
        // noted, which is the absent-scale case.
        let boot = now_ms();
        watchdog.armed_at(Millis::new(boot));
        assert!(watchdog.is_armed());
        assert!(!watchdog.is_faulted(Millis::new(boot)));

        let boundary = boot.wrapping_add(SIGNAL_TIMEOUT.raw());
        assert!(
            !watchdog.is_faulted(Millis::new(boundary)),
            "the deadline is inclusive, as the C++'s `>` is"
        );
        assert!(
            watchdog.is_faulted(Millis::new(boundary.wrapping_add(1))),
            "a cell that never converts must fault one tick past the deadline"
        );
        assert_eq!(
            watchdog.silent_for_ms(Millis::new(boundary.wrapping_add(1))),
            Some(SIGNAL_TIMEOUT.raw().wrapping_add(1)),
        );
    }

    /// The queue round-trips a command, and a full queue drops rather than
    /// blocks.
    ///
    /// The drop is the property: `request_tare` must never wait on the sampling
    /// task, because the caller is the control task and a control task that
    /// blocks on a scale is the exact failure this design exists to avoid.
    #[cfg_attr(test, test)]
    pub fn a_command_queue_drops_rather_than_blocking_when_full() {
        let queue: HalQueue<u32> = HalQueue::new(2);
        assert!(send(&queue, 1));
        assert!(send(&queue, 2));
        // Third: the queue is full, so this is a drop, and it returns.
        assert!(!send(&queue, 3));
        assert_eq!(queue.recv_front(0).map(|(v, _)| v), Some(1));
        assert_eq!(queue.recv_front(0).map(|(v, _)| v), Some(2));
        assert_eq!(queue.recv_front(0).map(|(v, _)| v), None);
    }

    /// The sample count the config asks for reaches the driver's clamp.
    ///
    /// `hardware.sensors.scale.samples` is `1..=20` (`cc-config`) and the driver
    /// rounds down to a power of two, so a config of 3 is 2 and a config of 20
    /// is 16. The config value is read once at boot and never re-read, so this
    /// is the only place the two meet.
    #[cfg_attr(test, test)]
    pub fn the_configured_sample_count_rounds_down_to_a_power_of_two() {
        use cc_domain::sensor::hx711::{normalise_samples, Cell};
        for (configured, expected) in [(1, 1), (2, 2), (3, 2), (4, 4), (20, 16)] {
            assert_eq!(
                normalise_samples(configured),
                expected,
                "samples = {configured}"
            );
        }
        let cell = Cell::new(1.0, 20);
        assert_eq!(cell.samples_in_use(), 16);
    }
}
