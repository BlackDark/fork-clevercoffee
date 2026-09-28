//! The heater output: LEDC hardware PWM, behind one swappable type.
//!
//! Owner: **R1-07**.
//!
//! # What this replaces
//!
//! The C++ chops the heater relay in a 10 ms hardware-timer ISR
//! (`include/clevercoffee/isr.h:85-118`), at the highest interrupt priority the
//! chip offers, and that ISR is the only hard-real-time requirement in the whole
//! firmware:
//!
//! ```cpp
//! if (currentPidOutput <= currentCounter) relay->off(); else relay->on();
//! unsigned int newCounter = currentCounter + 10;
//! if (newCounter >= ctx->processWindowSize()) newCounter = 0;
//! ```
//!
//! Everything *about* that decision — the window, the 10 ms quantisation, the
//! gate — is in [`cc_domain::heater`], where it is host-testable. This file owns
//! exactly one thing: the pin, and the peripheral that drives it.
//!
//! # LEDC, and why
//!
//! [`hal::ledc`] generates the carrier in hardware. There is no ISR, no CPU, no
//! scheduler dependency, and nothing to jitter: the duty is a register.
//!
//! | | C++ ISR chopper | LEDC |
//! | --- | --- | --- |
//! | on-time resolution | 10 ms (1 % of the window) | 1/131 072 of the carrier period (7.6 µs) |
//! | delivered power | `on_ticks / 100` | identical, by construction |
//! | CPU per second | 100 interrupt entries | 0 |
//! | worst-case latency | one interrupt-priority preemption | n/a — the duty *is* the output |
//! | failure mode if the scheduler stalls | heater runs at the last commanded duty | unchanged (the register holds its value) |
//! | **contactor level changes per second** | **0 or 2** | **0 or 2** — must match, see below |
//!
//! # The carrier frequency: 1 Hz, and it has to be low
//!
//! **The pin is a contactor, not a GPIO.** A carrier's frequency is a
//! *mechanical* duty on the relay, not a fidelity setting, and the number that
//! matters is how many times the level changes, not how smoothly it does.
//!
//! The C++ ISR runs 100 times a second and that is **not** a 100 Hz chopper.
//! Re-asserting the same level is not a transition. Walking `isr.h:96-118` for a
//! constant `pidOutput` over one 1000 ms window — the predicate
//! `pidOutput > counter` is monotone, so the on-time is one contiguous run
//! starting at counter 0 — gives:
//!
//! | `pid_output` | ISR entries/s | on-ticks | on-time | level over the window | level changes/s |
//! | --- | --- | --- | --- | --- | --- |
//! | 0 | 100 | 0 | 0.0 % | OFF throughout | **0** |
//! | 50 | 100 | 5 | 5 % | ON 50 ms, OFF 950 ms | **2** |
//! | 500 | 100 | 50 | 50 % | ON 500 ms, OFF 500 ms | **2** |
//! | 950 | 100 | 95 | 95 % | ON 950 ms, OFF 50 ms | **2** |
//! | 1000 | 100 | 100 | 100 % | ON throughout | **0** |
//!
//! Two changes per second: one falling edge inside the window and one rising
//! edge at the wrap. An `f` Hz square wave makes `2f`, so **`f ≤ 1 Hz`**, and the
//! only frequency at which the duty also keeps its meaning — one period *is* one
//! control window — is exactly `1 / WINDOW_MS`. Hence
//! [`heater::CARRIER_HZ`](cc_domain::heater::CARRIER_HZ) = 1.
//!
//! A 100 Hz carrier would deliver the same *average* power to within a rounding
//! error of a few thousandths of a percent, and spend **two hundred** contactor
//! operations per second where the C++ spent two. On a 2 kW boiler contactor,
//! whose mechanical life is counted in operations, that is a defect dressed up
//! as fidelity. The duty-resolution argument runs the other way from the usual
//! one: a *low* carrier makes the duty step coarser, so the low carrier has to
//! be paid for in bits, and the next section is about that.
//!
//! ## Why 17 bits, from ESP-IDF's own divider arithmetic
//!
//! The duty step at `f` Hz and `b` bits is `1000 / f / 2^b` ms. Matching the
//! C++'s 10 ms step needs `2^b ≥ 100 / f`, so at 1 Hz, `b ≥ 7` — but that is not
//! the binding constraint. `ledc_calculate_divisor`
//! (`esp_driver_ledc/src/ledc.c:459-477`) computes
//!
//! ```text
//! div_param = ((src_clk << 8) + freq_hz * precision / 2) / (freq_hz * precision)
//! ```
//!
//! with `precision = 1 << duty_resolution` (`ledc.c:600`), and
//! `LEDC_IS_DIV_INVALID` rejects `div_param <= LEDC_LL_FRACTIONAL_MAX` (255,
//! `hal/esp32/include/hal/ledc_ll.h:28`) or `> LEDC_TIMER_DIV_NUM_MAX`
//! (`0x3FFFF` = 262 143) (`ledc.c:115,111`). On the
//! original ESP32 the LEDC source is APB at 80 MHz (`LEDC_LL_GLOBAL_CLOCKS`
//! lists `LEDC_SLOW_CLK_APB` first and `esp-idf-hal` passes `LEDC_AUTO_CLK`).
//! The timer period is `div_param × 2^(bits − 8)` source clocks — that is
//! `(div_param >> 8)` whole precision ticks plus the 8-bit fractional remainder —
//! so:
//!
//! | bits | `div_param` at 1 Hz | valid? | `max_duty` | duty step | realised `f` |
//! | --- | --- | --- | --- | --- | --- |
//! | 7 | 160 000 000 | ✗ > `0x3FFFF` | 128 | 7.81 ms | — |
//! | 16 | 312 500 | ✗ > `0x3FFFF` | 65 536 | 15.3 µs | — |
//! | **17 (chosen)** | **156 250** | **✓** | **131 072** | **7.63 µs** | **exactly 1.000 000 Hz** |
//! | 18 | 78 125 | ✓ | 262 144 | 3.81 µs | exactly 1.000 000 Hz |
//! | 19 | 39 063 | ✓ | 524 288 | 1.91 µs | 0.999 987 Hz |
//! | 20 | 19 531 | ✓ | 1 048 575 | 0.95 µs | 1.000 013 Hz |
//!
//! So at 1 Hz the reachable resolutions are **17, 18, 19 and 20 and nothing
//! coarser**: 16 bits already overflows the maximum divider, and every bit below
//! that overflows it further. [`heater::CHOSEN_RESOLUTION_BITS`] is 17 — the
//! **coarsest that works**, which is the right way round because it leaves the
//! most margin against the divider arithmetic being wrong, and because 19 and
//! 20 are the two that lose exact frequency to the `+ freq_hz * precision / 2`
//! rounding term. 19 bits is also perfectly acceptable and is the constant to
//! change if the margin ever needs to be wider.
//!
//! Concretely, `div_param = 156 250 = 610 × 256 + 50`, so the period is
//! `610.3515625 × 131 072` = **80 000 000 APB clocks = 1.000 000 s exactly**.
//! If `LEDC_AUTO_CLK` were to fall through to `RC_FAST` (≈8 MHz) instead of APB,
//! `div_param` would be 15 625 and the period 8 000 000 `RC_FAST` clocks — also
//! exactly 1 s. The frequency does not depend on which of the two the driver
//! picks.
//!
//! ### The duty ends, and why 17 rather than 20
//!
//! `esp-idf-hal`'s `Resolution::max_duty` is `2^N`, **except** at 20 bits where it
//! is `2^20 - 1`, and `ledc_channel_config` says why, in its own comment:
//!
//! > On ESP32 … due to a hardware bug, 100 % duty cycle (i.e. `2**duty_res`) is
//! > not reachable when the binded timer selects the maximum duty resolution.
//!
//! The maximum low-speed resolution on the ESP32 is 20 bits
//! (`SOC_LEDC_TIMER_BIT_WIDTH`, `soc/esp32/include/soc/soc_caps.h:246`, and
//! `ledc_timer_config` rejects `duty_resolution >= LEDC_TIMER_BIT_MAX`,
//! `ledc.c:791`), so at `Bits20` full
//! power would be `1 048 575 / 1 048 576` — a permanent 0.95 ms notch once a
//! second, and "100 % duty" would be a lie the register could not express. At
//! 17 bits, `max_duty` is a plain `131 072`, so:
//!
//! * **duty 0** is a *steady low level* — the idle level `ledc.c` configures —
//!   and it is what the gate produces for a closed gate, so "disabled" and
//!   "off" are the same register value;
//! * **duty `max_duty`** is a *steady high level*, a different register value
//!   from 0, so full power is never confused with disabled;
//! * **any interior duty** is one high pulse per second, at `hpoint = 0` — i.e.
//!   starting at the beginning of the period, which is where the C++ puts it
//!   too (the relay is energised at counter 0 and de-energised as the counter
//!   passes `pidOutput`).
//!
//! `Resolution::Bits20` is a compile-time error here, not a code review note.
//!
//! ## The narrowest pulse is still the C++'s 10 ms
//!
//! 131 072 counts would allow a 7.6 µs pulse, and nothing ever asks for one: the
//! duty is `on_fraction(pid)`, and `on_fraction` is `chopper_on_ticks / 100`
//! with the C++'s 10 ms quantisation. The smallest non-zero duty is therefore one
//! whole 10 ms step, exactly the narrowest pulse the C++ has ever produced.
//! `cc_domain::heater::tests::the_minimum_pulse_is_the_cpp_ten_millisecond_step`
//! walks the whole range to prove it.
//!
//! # Low speed mode, not high
//!
//! The original ESP32 is the only chip with LEDC high-speed mode
//! (`esp-idf-hal-0.47.0/src/ledc.rs`, `#[cfg(esp32)] pub struct HighSpeed`).
//! **It is not needed.** High speed exists for the ≥ 10 MHz carrier duty that
//! low speed cannot produce; at 1 Hz low speed is six orders of magnitude inside
//! its range. Taking high speed would buy nothing and would consume one of the
//! four high-speed timers, which is the scarce resource on the original ESP32 —
//! it has four of each, and the other peripherals on this board want them too.
//! The decision is recorded rather than left implicit: if the carrier ever has
//! to go above ~1 MHz, this is the line to change.
//!
//! # 🔴 Blocked on hardware: the 1 Hz carrier trips the interrupt watchdog
//!
//! Measured 2026-09-28 on the attached board. **This driver panics the chip at
//! boot**, and the cause is ESP-IDF's own HAL, not this file:
//!
//! ```c
//! // components/hal/esp32/include/hal/ledc_ll.h:485-489, ESP-IDF v5.5.5
//! // wait until the last duty change took effect (duty_start bit will be
//! // self-cleared when duty update or fade is done)
//! // this is necessary on ESP32 only, otherwise, internal logic might mess up
//! while (hw->channel_group[speed_mode].channel[channel_num].conf1.duty_start);
//! ```
//!
//! `duty_start` is cleared by the hardware at the next **timer period**, and the
//! spin is inside `portENTER_CRITICAL(&ledc_spinlock)`
//! (`components/esp_driver_ledc/src/ledc.c:1603-1606`) — interrupts masked. At
//! the 1 Hz carrier chosen below, that is up to **one second**. The original
//! ESP32's interrupt watchdog is **300 ms** (`components/esp_system/int_wdt.c`).
//!
//! The 1 Hz choice is still right *for the contactor* and the argument below is
//! still sound; it is simply not compatible with this chip's LEDC driver, which
//! nobody checked. The boot backtrace decodes to `ledc_set_duty_and_update` ->
//! `ledc_ll_set_duty_start`.
//!
//! Until it is resolved, `cc-firmware` leaves `BRING_UP_HEATER_LEDC` false and
//! holds GPIO2 as a plain inactive output. See
//! [09-cpp-findings.md §20](../../../docs/rust-migration/09-cpp-findings.md).
//!
//! # ⚠ Not yet exercised on hardware, and the contactor is still unknown
//!
//! **The heater has never been energised by this code, and must not be until
//! R1-07's safe test procedure has been run with the boiler disconnected.** What
//! *is* verified is the arithmetic (host tests in `cc_domain::heater`), the
//! divider feasibility (the table above, read out of ESP-IDF v5.5.5's own
//! source), and that the firmware builds and boots with the carrier configured
//! at duty 0.
//!
//! What is **not** verified, and cannot be verified from a datasheet-free desk:
//!
//! * **The contactor's minimum on-time and minimum off-time.** The software
//!   guarantees it never asks for a pulse narrower than 10 ms, because that is
//!   the C++'s own quantisation. Whether 10 ms is *itself* long enough is a
//!   measurement.
//! * **Whether a hardware-PWM output is acceptable to the coil at all.** LEDC
//!   drives a square wave into the same pin the C++ drove. 1 Hz is a frequency
//!   the C++ never produced, so the coil's behaviour at it is unknown even
//!   though the C++'s 1 Hz *average* is unchanged.
//! * **The realised frequency and duty on the pin.** No scope has been attached.
//!
//! The duty-versus-time measurement against a dummy load — R1-07 steps 1 and 2 —
//! is **not** done, and **R1-07 stays open**. See
//! `docs/rust-migration/intentional-diffs.md` #5.

use core::marker::PhantomData;

use cc_domain::heater::{self, GateBlock, HeaterGate};
use cc_domain::units::{Duty, Millis};
use esp_idf_hal::gpio::OutputPin;
use esp_idf_hal::ledc::config::TimerConfig;
use esp_idf_hal::ledc::{LedcChannel, LedcDriver, LedcTimer, LedcTimerDriver, Resolution};
use esp_idf_hal::units::Hertz;
use esp_idf_svc::sys::EspError;

/// The carrier frequency. See the module docs for why this and not another.
///
/// It is [`cc_domain::heater::CARRIER_HZ`] and not a literal, because the number
/// that matters is the contactor's mechanical duty and the host tests in
/// `cc_domain::heater` are where that is proved. **1 Hz, not 100 Hz**: the C++
/// runs its ISR 100 times a second but changes the relay level twice.
pub const CARRIER_HZ: Hertz = Hertz(cc_domain::heater::CARRIER_HZ);

/// The timer resolution. See the module docs; `Bits17` is the coarsest that
/// reaches 1.000 000 Hz on the original ESP32, and the coarsest that is not the
/// maximum resolution — which is the one ESP-IDF documents as unable to reach
/// 100 % duty on this chip.
pub const RESOLUTION: Resolution = Resolution::Bits17;

// The chosen resolution, the `max_duty` the domain crate's host tests use, and
// the peripheral's own answer have to be the same number. This is where a
// hardware detail that lives in `esp-idf-hal` and a test that lives in
// `cc-domain` are made to agree, at compile time, on every build of the device
// crate — rather than in a comment that can rot.
//
// The `2^N - 1` clause is what makes this worth a compile-time assert rather
// than a test: `Resolution::max_duty()` is `2^20 - 1` at `Bits20`, and
// `ledc_channel_config`'s own comment says 100 % duty "is not reachable when the
// binded timer selects the maximum duty resolution" on the ESP32. Reaching
// `Resolution::Bits20` here would silently turn full power into a permanent
// 0.95 ms notch, and it would do it in a way the host tests would not see,
// because they are parameterised on `cc_domain::heater::CHOSEN_MAX_DUTY`.
const _: () = assert!(
    RESOLUTION.bits() == cc_domain::heater::CHOSEN_RESOLUTION_BITS,
    "RESOLUTION and cc_domain::heater::CHOSEN_RESOLUTION_BITS disagree"
);
const _: () = assert!(
    RESOLUTION.max_duty() == cc_domain::heater::CHOSEN_MAX_DUTY,
    "RESOLUTION and cc_domain::heater::CHOSEN_MAX_DUTY disagree"
);

/// The one thing `HeaterOutput` needs from whatever drives the pin.
///
/// This trait is the seam that makes the R1-07 decision swappable (04 §5). Today
/// exactly one implementation exists — [`LedcPwm`] — and the GPTimer-ISR
/// fallback is deliberately *not* written yet, because writing a second
/// implementation of an interface nobody has switched to is how untested code
/// gets shipped. When the hardware test says LEDC is not acceptable, this is
/// the trait a `TimerIsrPwm` implements and nothing else changes.
///
/// It is deliberately tiny: one method, one integer. Everything decidable —
/// the window, the quantisation, the gate — is decided in [`cc_domain::heater`]
/// and arrives here already reduced to a count.
pub trait HeaterDuty {
    /// Drive the pin at `counts` out of `max_duty`.
    ///
    /// `counts` is already clamped by the caller; an implementation may clamp
    /// again, and `esp-idf-hal` does, but it must not be relied on.
    ///
    /// # Errors
    ///
    /// Whatever the underlying peripheral reports. For [`LedcPwm`] that is
    /// `ledc_set_duty_and_update`, which fails only if the peripheral is not
    /// initialised — a state this crate cannot be in after a successful
    /// [`LedcPwm::new`]. The duty is **not** changed on failure, and the caller
    /// must treat an error as "the previous value is still on the pin", never as
    /// "the pin is at zero".
    fn apply(&mut self, counts: u32, max_duty: u32) -> Result<(), EspError>;
}

/// LEDC hardware PWM. The R1-07 decision.
///
/// # Why the timer driver is stored
///
/// `LedcDriver::new` takes the timer driver by `Borrow` and does not keep it,
/// so it is tempting to pass a temporary. That is a latent bug: `LedcTimerDriver`
/// has a `Drop` that **resets the timer** (`esp-idf-hal-0.47.0/src/ledc.rs`,
/// `impl Drop for LedcTimerDriver`), so a temporary would stop the carrier the
/// instant the constructor returned. The timer driver is therefore owned here
/// for as long as the channel that depends on it.
pub struct LedcPwm<'d, C>
where
    C: LedcChannel,
{
    driver: LedcDriver<'d>,
    _timer: LedcTimerDriver<'d, C::SpeedMode>,
    _channel: PhantomData<C>,
    max_duty: u32,
}

impl<'d, C> LedcPwm<'d, C>
where
    C: LedcChannel,
{
    /// Configure a timer at [`CARRIER_HZ`] / [`RESOLUTION`], attach `pin` to
    /// `channel`, and leave the output at duty 0.
    ///
    /// **Duty 0 is not an initialisation detail, it is the safe state.** The
    /// channel is configured with `duty: 0, hpoint: 0` and the idle level is
    /// `0` (`ledc.c`'s `IDLE_LEVEL`), so a `HIGH_TRIGGER` relay is
    /// de-energised from the moment this returns — and stays de-energised if this
    /// function is never called again, because the register holds 0.
    /// The timer and the channel must agree on the speed mode: `LedcDriver::new`
    /// takes a `LedcTimerDriver<'d, C::SpeedMode>` and there is no conversion
    /// between a low-speed and a high-speed timer, because on the original ESP32
    /// they are different peripherals on different clock trees. The bound is
    /// therefore in the signature rather than checked at run time.
    ///
    /// # Errors
    ///
    /// Whatever `ledc_timer_config` or `ledc_channel_config` reports. The most
    /// likely one on this chip is the carrier frequency being unreachable at the
    /// requested resolution, which would mean [`CARRIER_HZ`] and [`RESOLUTION`]
    /// have been changed into a pair ESP-IDF's divider arithmetic cannot reach;
    /// the module docs' table is the thing to re-derive if that happens.
    ///
    /// # Panics
    ///
    /// If `TimerConfig::frequency` / `::resolution` do not carry the requested
    /// values through, which would mean a change in `esp-idf-hal`. A panic at
    /// boot, before the pins are trusted, is the correct outcome: it is better
    /// than running a heater at a frequency nobody measured.
    pub fn new<T>(channel: C, timer: T, pin: impl OutputPin + 'd) -> Result<Self, EspError>
    where
        C: LedcChannel + 'd,
        T: LedcTimer<SpeedMode = C::SpeedMode> + 'd,
    {
        // `TimerConfig` is what `ledc_timer_config` takes verbatim: the frequency
        // goes to `freq_hz` and the resolution to `duty_resolution`
        // (`esp-idf-hal-0.47.0/src/ledc.rs:93-140`). Both are the values the
        // module docs measured, and both are asserted rather than assumed.
        let config = TimerConfig::new()
            .frequency(CARRIER_HZ)
            .resolution(RESOLUTION);
        assert_eq!(
            config.frequency, CARRIER_HZ,
            "TimerConfig::frequency must carry the carrier frequency through"
        );
        assert_eq!(
            config.resolution, RESOLUTION,
            "TimerConfig::resolution must carry the resolution through"
        );

        let timer_driver: LedcTimerDriver<'d, C::SpeedMode> = LedcTimerDriver::new(timer, &config)?;
        let mut driver = LedcDriver::new(channel, &timer_driver, pin)?;
        let max_duty = driver.get_max_duty();
        driver.set_duty(0)?;

        Ok(Self {
            driver,
            _timer: timer_driver,
            _channel: PhantomData,
            max_duty,
        })
    }

    /// The resolution's `max_duty`, i.e. the count that means 100 %.
    #[must_use]
    pub const fn max_duty(&self) -> u32 {
        self.max_duty
    }

    /// The count currently in the channel's duty register, read back from the
    /// driver rather than from [`HeaterOutput::applied_duty`].
    ///
    /// This is what R1-07's hardware test asserts against a scope: the value the
    /// hardware believes, not the value the software thinks it wrote.
    #[must_use]
    pub fn hardware_duty(&self) -> u32 {
        self.driver.get_duty()
    }
}

impl<C> HeaterDuty for LedcPwm<'_, C>
where
    C: LedcChannel,
{
    fn apply(&mut self, counts: u32, max_duty: u32) -> Result<(), EspError> {
        // Clamp before the register, not after. `LedcDriver::set_duty` clamps to
        // `self.get_max_duty()` silently, so an unclamped request is not an
        // error — it is a wrong-but-plausible duty on a heater.
        self.driver
            .set_duty(counts.min(max_duty.min(self.max_duty)))
    }
}

/// The heater output: a [`HeaterDuty`] plus the latching gate.
///
/// This is the *type* 04 §5 asks for — "the decision (LEDC vs `GPTimer`) is
/// abstracted behind one type so it is swappable". `HeaterOutput<P>` is that
/// type; the transport is the type parameter.
///
/// # The gate is not optional
///
/// `HeaterOutput::set_duty` is the only way to move the pin above zero, and it
/// consults the [`HeaterGate`] first. That is the whole of the recovered
/// firmware's `"output held off until the supervisor beats"`
/// ([08 §3](../../docs/rust-migration/08-recovered-oracle.md)) and its deadman
/// ([08 §4](../../docs/rust-migration/08-recovered-oracle.md)): there is no
/// method that writes a non-zero duty without passing the gate, so a new caller
/// cannot bypass it.
pub struct HeaterOutput<P> {
    transport: P,
    gate: HeaterGate,
    max_duty: u32,
    /// The duty the machine last asked for, before the gate.
    requested: u32,
    /// The duty the gate actually let through.
    applied: u32,
}

impl<'d, C> HeaterOutput<LedcPwm<'d, C>>
where
    C: LedcChannel,
{
    /// Wrap an LEDC transport, taking `max_duty` from its timer resolution.
    ///
    /// The gate starts **closed** — `HeaterGate::new` is the only constructor,
    /// so there is no way to build an armed one by accident.
    #[must_use]
    pub const fn new_ledc(transport: LedcPwm<'d, C>) -> Self {
        Self {
            max_duty: transport.max_duty(),
            transport,
            gate: HeaterGate::new(),
            requested: 0,
            applied: 0,
        }
    }
}

impl<P: HeaterDuty> HeaterOutput<P> {
    /// Wrap a transport whose resolution the caller knows.
    pub const fn new(transport: P, max_duty: u32) -> Self {
        Self {
            transport,
            gate: HeaterGate::new(),
            max_duty,
            requested: 0,
            applied: 0,
        }
    }

    /// The gate, so the supervisor task can beat into it.
    pub const fn gate(&mut self) -> &mut HeaterGate {
        &mut self.gate
    }

    /// The transport, for the R1-07 hardware test's readback.
    pub const fn transport(&self) -> &P {
        &self.transport
    }

    /// The timer resolution's `max_duty`.
    #[must_use]
    pub const fn max_duty(&self) -> u32 {
        self.max_duty
    }

    /// The duty the gate last let through, for the periodic log line.
    #[must_use]
    pub const fn applied_duty(&self) -> u32 {
        self.applied
    }

    /// The duty the machine last asked for, whether or not the gate allowed it.
    #[must_use]
    pub const fn requested_duty(&self) -> u32 {
        self.requested
    }

    /// Why the output is at zero, or `None` if it is not being held down.
    #[must_use]
    pub fn blocked_at(&self, now: Millis) -> Option<GateBlock> {
        self.gate.blocked_at(now)
    }

    /// Drive a heater duty, subject to the gate.
    ///
    /// `pid_output` is the C++'s millisecond duty in a 1000 ms window
    /// ([`cc_domain::units::Duty`]). The order is: quantise to the chopper's
    /// 10 ms step, convert to a count at this timer's resolution, then gate.
    ///
    /// Gating **last** is deliberate. If it ran first, a "permit" would be
    /// cached and a heartbeat that arrived between the decision and the write
    /// would not be seen. Gating at the last possible moment means the value in
    /// the register is at most one interlock period stale, and the worst case is
    /// stale-low, never stale-high.
    ///
    /// # Errors
    ///
    /// Whatever the transport reports, in which case **the register still holds
    /// the previous duty**. A caller must not conclude the heater is off.
    pub fn set_duty(&mut self, now: Millis, pid_output: Duty) -> Result<u32, EspError> {
        let requested = heater::duty_counts(pid_output, self.max_duty);
        self.requested = requested;
        let applied = self.gate.resolve(now, requested, self.max_duty);
        self.applied = applied;
        self.transport.apply(applied, self.max_duty)?;
        Ok(applied)
    }

    /// Drive a raw count, subject to the gate.
    ///
    /// The form R1-07's dummy-load test uses: it puts a known count on the pin
    /// so a scope can measure the carrier, without going through the PID's
    /// millisecond scale. Still gated — a raw count is not an exemption.
    ///
    /// # Errors
    ///
    /// As [`Self::set_duty`]: the register holds the previous duty.
    pub fn set_raw_counts(&mut self, now: Millis, counts: u32) -> Result<u32, EspError> {
        self.requested = counts.min(self.max_duty);
        let applied = self.gate.resolve(now, self.requested, self.max_duty);
        self.applied = applied;
        self.transport.apply(applied, self.max_duty)?;
        Ok(applied)
    }

    /// The heater's on-time fraction, for the oracle's `on_fraction=` log field
    /// ([08 §4](../../docs/rust-migration/08-recovered-oracle.md)).
    #[must_use]
    pub fn on_fraction(pid_output: Duty) -> f64 {
        heater::on_fraction(pid_output)
    }
}
