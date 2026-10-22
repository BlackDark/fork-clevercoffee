//! Sole facade for pump, shared valve relay, and heater duty.
//!
//! Effects reach pins through the applier; the heater ISR owns the physical pin.
//! Interlocks are checked inside methods so callers cannot bypass them:
//!
//! * Emergency shutdown latches; ordinary safe shutdown does not. The safety reducer
//!   owns the latch and supplies its value each tick through [`Actuators::set_latched`].
//! * An empty tank blocks both pump and water valve.
//! * Steam and water share one relay. Both open paths enforce their safety whitelist;
//!   closing one valve must preserve the other's request.
//!
//! Set the cached state after reducing and before applying effects, including MQTT
//! effects. Otherwise a valid transition's valve opening is judged against the old state.
//!
//! [`Inhibit`] is a logged, counted bring-up override, not a safety mechanism. Set it
//! once at boot; effects must not change it or silently invalidate actuator tests.
//! See [pins.md](../../../docs/hardware/pins.md).
use cc_domain::heater::HeaterGate;
use cc_domain::state::MachineState;
use cc_domain::units::{Duty, Millis};
use cc_machine::{Actuators as ActuatorsTrait, Diagnostics, MachineChannels};
use esp_idf_hal::gpio::{InputOutput, Level, PinDriver};
use log::{debug, info, warn};

use crate::heater::HeaterOutput;

/// Which pin level means "energised" for one relay.
///
/// Every relay's `hardware.relays.*.trigger_type` decides which level energises
/// it, and this type is that branch, per relay. Two module-level constants used
/// to carry it instead, so `hardware.relays.pump.trigger_type` was **read by
/// nothing** anywhere in the workspace and a `LOW_TRIGGER` pump or valve relay
/// was driven inverted — `enable_pump` de-energised — reachable over plain
/// `POST /api/parameters`.
///
/// A note on why honouring it is not the same as permitting it: a low-trigger
/// relay **energises whenever its pin floats**, which is before any firmware
/// runs. That hazard is not specific to the heater, so
/// `cc_safety::validate_config` refuses `LOW_TRIGGER` for every relay, and this
/// type exists so the setting is not a lie when a configuration does carry one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Polarity {
    /// The level that energises the relay.
    pub active: Level,
    /// The level that de-energises it.
    pub inactive: Level,
}

impl Polarity {
    /// A relay energised by a high level — `HIGH_TRIGGER`, the default in both
    /// firmwares.
    #[must_use]
    pub const fn high_trigger() -> Self {
        Self {
            active: Level::High,
            inactive: Level::Low,
        }
    }

    /// A relay energised by a low level.
    #[must_use]
    pub const fn low_trigger() -> Self {
        Self {
            active: Level::Low,
            inactive: Level::High,
        }
    }

    /// The polarity for a configured trigger type.
    #[must_use]
    pub const fn for_trigger(trigger: cc_domain::hardware::RelayTriggerType) -> Self {
        match trigger {
            cc_domain::hardware::RelayTriggerType::LowTrigger => Self::low_trigger(),
            cc_domain::hardware::RelayTriggerType::HighTrigger => Self::high_trigger(),
        }
    }
}

/// The polarity of a `HIGH_TRIGGER` relay: active high.
///
/// Kept as a name because the bring-up sequence asserts its pin readback against
/// it, and because it is the default everywhere. The relays themselves carry
/// their own [`Polarity`]; this is the value a caller uses when it has no
/// configuration to hand.
pub const HIGH_TRIGGER: Polarity = Polarity::high_trigger();

/// The three relays' polarities, from the configuration.
///
/// One argument rather than three so a caller cannot wire two relays and forget
/// the third, and so `Actuators::new` stays a single obvious construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelayPolarities {
    /// `hardware.relays.pump.trigger_type`.
    pub pump: Polarity,
    /// `hardware.relays.valve.trigger_type`.
    pub valve: Polarity,
    /// `hardware.relays.heater.trigger_type`.
    ///
    /// Carried so the caller can report it, but **not applied to the pin**: the
    /// heater's write is inside the 10 ms ISR closure in
    /// [`crate::heater::TimerIsrPwm`], and `cc_safety::validate_config` refuses
    /// `LOW_TRIGGER` for the heater, so a polarity here could never change a
    /// behaviour. Threading a field into an ISR to honour a value that cannot
    /// reach it is complexity with no reachable effect; the refusal is the real
    /// mechanism and it lives in `validate_config`.
    pub heater: Polarity,
}

impl RelayPolarities {
    /// Every relay `HIGH_TRIGGER` — the default in both firmwares, and the only
    /// configuration `cc_safety::validate_config` accepts for any of them.
    #[must_use]
    pub const fn all_high_trigger() -> Self {
        Self {
            pump: Polarity::high_trigger(),
            valve: Polarity::high_trigger(),
            heater: Polarity::high_trigger(),
        }
    }

    /// Read the three from a configuration.
    #[must_use]
    pub const fn from_config(relays: &cc_config::config::HardwareRelays) -> Self {
        Self {
            pump: Polarity::for_trigger(relays.pump.trigger_type),
            valve: Polarity::for_trigger(relays.valve.trigger_type),
            heater: Polarity::for_trigger(relays.heater.trigger_type),
        }
    }
}

/// The heater transport, as this module names it.
type Heater = HeaterOutput;

/// Which valve(s) the shared relay is open for.
///
/// The four values are load-bearing: the relay is driven **on** for every value
/// except `Closed`, and collapsing this to a `bool` would lose the ability to
/// express "steam open, water closed" — which is the state `SteamRunning`'s
/// water-injection path needs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ValveState {
    /// Both closed — the relay is off.
    #[default]
    Closed,
    /// Only the steam valve open.
    SteamOpen,
    /// Only the water (three-way) valve open.
    WaterOpen,
    /// Both open.
    BothOpen,
}

impl ValveState {
    /// Whether the shared relay must be energised.
    ///
    /// The relay is energised for every value except [`ValveState::Closed`].
    #[must_use]
    pub const fn relay_should_be_on(self) -> bool {
        !matches!(self, Self::Closed)
    }
}

/// The interlock state [`Actuators`] consults, as a plain value.
///
/// Split out from [`Actuators`] for one reason: it is **pure**, so the safety
/// rules can be tested on the device by `just test-esp32` without a
/// `PinDriver`, a relay, or a machine that could be damaged by getting an answer
/// wrong. The tests are `actuators::tests::*` and they are registered in
/// [`crate::device_tests::CASES`].
///
/// It is a value rather than three fields on [`Actuators`] for the same reason
/// `cc_safety::Telemetry` is: the thing being decided is "may this actuator be
/// energised", and one type named after that question is auditable where three
/// booleans consulted in six methods are not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interlock {
    /// While set, nothing may be energised.
    pub latched: bool,
    /// While false, neither the pump nor the water valve may be energised.
    pub water_tank_full: bool,
    /// The machine's state, which S5's whitelist is a function of.
    pub state: MachineState,
    /// A `test_only` inhibit, held for the life of the process.
    pub inhibit: Inhibit,
}

impl Interlock {
    /// Nothing latched, a full tank, a machine in `PID_NORMAL`, nothing
    /// inhibited. The boot value.
    #[must_use]
    pub const fn healthy() -> Self {
        Self {
            latched: false,
            // Assume full, so a machine with no float switch fitted does not
            // refuse to pump on the first tick. The shell overwrites this from the
            // sensor or, when no sensor is fitted, leaves it here.
            water_tank_full: true,
            state: MachineState::Init,
            inhibit: Inhibit::NONE,
        }
    }

    /// S2: may the pump be energised?
    #[must_use]
    pub const fn may_pump(self) -> bool {
        !self.latched && self.water_tank_full && !self.inhibit.pump
    }

    /// S2 + S4 + S5: may the water valve be energised?
    ///
    /// The tank condition **and** S5's whitelist both apply here, which is the
    /// symmetry with [`Self::may_open_steam`]. S5 here is **belt-and-braces**
    /// rather than the only enforcement point: `cc_machine::tick` appends
    /// `Effect::CloseWaterValve` whenever the state is not whitelisted
    /// (`cc-machine/src/lib.rs:237`) and the applier runs the list in order, so
    /// the trailing close wins — but that is an ordering argument, and the MQTT
    /// effect path (`main.rs`) applies its own inbound commands **after** the
    /// tick's tail. The argument holds only because no `Command` emits
    /// `OpenWaterValve` (`handlers::apply_command` pushes `SetPidRuntime`,
    /// `RequestReboot` and `ResetStandbyTimer` and nothing else), which is a fact
    /// about one function that nothing enforces. Steam and water share one relay,
    /// so the asymmetry bought nothing.
    #[must_use]
    pub const fn may_open_water(self) -> bool {
        !self.latched
            && self.water_tank_full
            && cc_safety::water_flow_allowed(self.state)
            && !self.inhibit.valve
    }

    /// S2 + S5': may the steam valve be energised?
    #[must_use]
    pub const fn may_open_steam(self) -> bool {
        !self.latched && cc_safety::steam_flow_allowed(self.state) && !self.inhibit.valve
    }

    /// S2: may the heater carry a duty?
    #[must_use]
    pub const fn may_heat(self) -> bool {
        !self.latched && !self.inhibit.heater
    }
}

/// A `test_only` inhibit: refuse to energise, loudly.
///
/// **Not a safety mechanism.** Everything here is also enforced by
/// [`Interlock`], which is the real thing. An inhibit exists so a bring-up build
/// can hold a relay off while the code path that would drive it is exercised and
/// proven.
///
/// The three flags are separate because the three are proven separately: a
/// bring-up run holds the pump and both valves inhibited and the heater live, and
/// an inhibit that could not say "not the heater" could not express that.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inhibit {
    /// Refuse to energise the pump.
    pub pump: bool,
    /// Refuse to energise the valve relay.
    pub valve: bool,
    /// Refuse to give the heater a non-zero duty.
    pub heater: bool,
}

impl Inhibit {
    /// Nothing inhibited.
    pub const NONE: Self = Self {
        pump: false,
        valve: false,
        heater: false,
    };

    /// Every actuator held off: nothing is energised, so the state machine, the
    /// PID and the interlocks can all be exercised and no water or heat can move.
    pub const ALL: Self = Self {
        pump: true,
        valve: true,
        heater: true,
    };

    /// Whether any flag is set, for the boot log line.
    #[must_use]
    pub const fn any(self) -> bool {
        self.pump || self.valve || self.heater
    }
}

/// The actuator facade.
///
/// Owns the pump pin, the shared valve relay and the heater, and nothing else in
/// the firmware owns any of them. Constructed once, in `bring_up`, and moved
/// into the control task — which is therefore the only thread that can change an
/// actuator pin, and the only one that can beat the heater's deadman.
pub struct Actuators {
    /// The pump relay, GPIO27. See [pins.md](../../../docs/hardware/pins.md).
    pump: PinDriver<'static, InputOutput>,
    /// The **shared** steam/water relay, GPIO17. See [`ValveState`] and
    /// [pins.md](../../../docs/hardware/pins.md).
    valve: PinDriver<'static, InputOutput>,
    /// The heater, behind its deadman gate.
    heater: Heater,
    /// Which level energises the pump relay. `Relay::on()`'s branch.
    pump_polarity: Polarity,
    /// Which level energises the shared steam/water relay.
    valve_polarity: Polarity,
    /// Which valve(s) the shared relay is open for.
    valve_state: ValveState,
    /// The interlock state every energise method consults.
    interlock: Interlock,
    /// The clock the shell last published, used for the heater gate.
    ///
    /// [`cc_machine::Actuators::set_heater_duty`] takes no clock argument —
    /// the trait is deliberately that small — and [`HeaterOutput::set_duty`]
    /// needs one. The shell therefore publishes it once per tick with
    /// [`Self::set_now`] before the applier runs, which is the only ordering in
    /// which the two can agree.
    now: Millis,
    /// Later shutdowns in one hold stay quiet.
    shutdown_announced: bool,
    /// How many energise requests the interlock refused, by actuator.
    ///
    /// Counted rather than swallowed: "the pump did not run" and "the pump was
    /// refused because the tank is empty" are different facts, and only one of
    /// them is visible in `/api/status`.
    refusals: Refusals,
}

/// Per-actuator refusal counts, reported in the periodic tick line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Refusals {
    /// `enable_pump` refused.
    pub pump: u32,
    /// `open_water_valve` refused.
    pub water_valve: u32,
    /// `open_steam_valve` refused.
    pub steam_valve: u32,
    /// `set_heater_duty` with a non-zero duty refused.
    pub heater: u32,
}

impl Actuators {
    /// Take ownership of the two relay pins and the heater.
    ///
    /// The pins must already be configured and driven inactive, and read back
    /// inactive, before this is called: the bring-up sequence's assertion is the
    /// last moment at which a pin can be read as a pin, and it runs first.
    #[must_use]
    pub const fn new(
        pump: PinDriver<'static, InputOutput>,
        valve: PinDriver<'static, InputOutput>,
        heater: Heater,
        polarity: RelayPolarities,
    ) -> Self {
        Self {
            pump,
            valve,
            heater,
            pump_polarity: polarity.pump,
            valve_polarity: polarity.valve,
            valve_state: ValveState::Closed,
            interlock: Interlock::healthy(),
            now: Millis::ZERO,
            shutdown_announced: false,
            refusals: Refusals {
                pump: 0,
                water_valve: 0,
                steam_valve: 0,
                heater: 0,
            },
        }
    }

    /// Apply the configured relay polarities.
    ///
    /// Called once at boot, after the configuration is loaded and **before** the
    /// control task exists — so no actuator write can race it. See the call
    /// site's comment for why the polarity is not known at construction.
    pub const fn set_relay_polarities(&mut self, polarity: RelayPolarities) {
        self.pump_polarity = polarity.pump;
        self.valve_polarity = polarity.valve;
    }

    /// The polarities in force.
    #[must_use]
    pub const fn relay_polarity(&self) -> RelayPolarities {
        RelayPolarities {
            pump: self.pump_polarity,
            valve: self.valve_polarity,
            heater: Polarity::high_trigger(),
        }
    }

    /// Publish the clock for this tick.
    ///
    /// Called by the shell immediately before `applier::apply`, so every
    /// `set_heater_duty` in one tick sees one reading.
    pub fn set_now(&mut self, now: Millis) {
        self.now = now;
    }

    /// The clock this tick published.
    #[must_use]
    pub const fn now(&self) -> Millis {
        self.now
    }

    /// Record the safety reducer's latch, once per tick.
    ///
    /// `cc_safety` owns the latch; this is the facade's mirror of it. See the
    /// module documentation for why it is not the other way round.
    pub const fn set_latched(&mut self, latched: bool) {
        self.interlock.latched = latched;
    }

    /// Record the water-tank float's reading, once per tick.
    pub const fn set_water_tank_full(&mut self, full: bool) {
        self.interlock.water_tank_full = full;
    }

    /// Record the machine state S5's whitelist is a function of, once per tick.
    pub const fn set_state(&mut self, state: MachineState) {
        self.interlock.state = state;
    }

    /// Replace the `test_only` inhibit. Called once, at boot, and logged.
    pub const fn set_inhibit(&mut self, inhibit: Inhibit) {
        self.interlock.inhibit = inhibit;
    }

    /// The `test_only` inhibit in force.
    #[must_use]
    pub const fn inhibit(&self) -> Inhibit {
        self.interlock.inhibit
    }

    /// The interlock state every energise method consults, for the boot log.
    #[must_use]
    pub const fn interlock(&self) -> Interlock {
        self.interlock
    }

    /// The hold ended. The next shutdown may log again.
    pub const fn release_shutdown_announcement(&mut self) {
        self.shutdown_announced = false;
    }

    /// The per-actuator refusal counts.
    #[must_use]
    pub const fn refusals(&self) -> Refusals {
        self.refusals
    }

    /// The valve relay's logical state.
    #[must_use]
    pub const fn valve_state(&self) -> ValveState {
        self.valve_state
    }

    /// The heater's deadman gate, so the control task can beat it.
    ///
    /// Public because the *only* legitimate caller is the supervisor, and
    /// `cc-firmware`'s control task is the supervisor. It exposes a
    /// heartbeat and nothing else that could open the gate from a different
    /// thread with a different cadence.
    pub const fn gate(&mut self) -> &mut HeaterGate {
        self.heater.gate()
    }

    /// The heater, for the transport's readback counters and the duty the gate
    /// let through. The duty may only be *set* through
    /// [`ActuatorsTrait::set_heater_duty`].
    #[must_use]
    pub const fn heater(&self) -> &Heater {
        &self.heater
    }

    /// Arm the 10 ms heater ISR, once the first heartbeat has been taken.
    pub fn arm_heater_isr(&self) {
        self.heater.transport().arm();
    }

    /// Stop the heater ISR driving.
    ///
    /// # Errors
    ///
    /// Never: there is no peripheral call. The pin belongs to the ISR and is
    /// already at zero because the gate closed the duty before this is reached.
    pub fn disarm_heater_isr(&self) -> Result<(), esp_idf_svc::sys::EspError> {
        self.heater.transport().disarm()
    }

    /// Whether the pin reads energised, for the periodic readback line.
    ///
    /// The pump and the valve only; the heater pin belongs to the ISR, and the
    /// honest readback for that one is the ISR's own record of what it drove
    /// ([`crate::heater::TimerIsrPwm::is_high`]).
    #[must_use]
    pub fn pins_read_active(&self) -> (bool, bool) {
        // **Polarity-aware.** It read `is_high()` as "energised", which is only
        // true for a high-trigger relay: on a low-trigger board the log would
        // report the inverse of reality, and `enable_pump`'s comment cites this
        // as the check.
        (
            self.pump.is_high() == (self.pump_polarity.active == Level::High),
            self.valve.is_high() == (self.valve_polarity.active == Level::High),
        )
    }

    /// Drive the valve relay from [`Self::valve_state`].
    ///
    /// No log line: the state is already logged by the transition that caused it,
    /// and a second line per tick per valve would bury the boot log.
    ///
    /// A pin write can fail, and a failure is **not** corrected here: the state
    /// variable has already moved, which is exactly the drift this type exists to
    /// keep visible. What this does instead is leave the state consistent with
    /// the *intent* and report the failure, so a stuck
    /// relay shows up as a recurring error rather than as a state variable that
    /// disagrees with the pin in a way nothing can see.
    fn update_valve_relay(&mut self) {
        let level = if self.valve_state.relay_should_be_on() {
            self.valve_polarity.active
        } else {
            self.valve_polarity.inactive
        };
        if let Err(err) = self.valve.set_level(level) {
            warn!(
                "actuators: the valve relay (GPIO17) could not be driven to {level:?}: \
                 {err:?} — the bookkeeping says {:?}",
                self.valve_state
            );
        }
    }
}

impl ActuatorsTrait for Actuators {
    fn enable_pump(&mut self) {
        if !self.interlock.may_pump() {
            self.refusals.pump = self.refusals.pump.saturating_add(1);
            warn!(
                "actuators: enablePump REFUSED — latched {}, tank_full {}, inhibited {}",
                self.interlock.latched, self.interlock.water_tank_full, self.interlock.inhibit.pump
            );
            return;
        }
        // Driving the relay is idempotent; the facade keeps no cached flag for the
        // pump because the write itself is the state and the pin readback in
        // `pins_read_active` is the check.
        if let Err(err) = self.pump.set_level(self.pump_polarity.active) {
            warn!("actuators: the pump relay (GPIO27) could not be driven: {err:?}");
        }
    }

    fn disable_pump(&mut self) {
        if let Err(err) = self.pump.set_level(self.pump_polarity.inactive) {
            warn!("actuators: the pump relay (GPIO27) could not be de-energised: {err:?}");
        }
    }

    fn open_water_valve(&mut self) {
        if !self.interlock.may_open_water() {
            self.refusals.water_valve = self.refusals.water_valve.saturating_add(1);
            warn!(
                "actuators: openWaterValve REFUSED — latched {}, tank_full {}, inhibited {}",
                self.interlock.latched,
                self.interlock.water_tank_full,
                self.interlock.inhibit.valve
            );
            return;
        }
        // `CLOSED -> WATER_OPEN`, `STEAM_OPEN -> WATER_OPEN`, and the two
        // already-water-open states are a no-op that still falls through to the
        // relay write. The steam arm mirrors this table and reaches `BOTH_OPEN`,
        // which neither arm produces in practice: nothing opens the water valve
        // while the steam valve is already open, because the two whitelists are
        // disjoint.
        self.valve_state = ValveState::WaterOpen;
        self.update_valve_relay();
    }

    fn close_water_valve(&mut self) {
        // A close does **not** check the latch: it must always be permitted, or a
        // latched machine could be left with the valve open.
        self.valve_state = match self.valve_state {
            ValveState::Closed | ValveState::WaterOpen => ValveState::Closed,
            ValveState::SteamOpen | ValveState::BothOpen => ValveState::SteamOpen,
        };
        self.update_valve_relay();
    }

    fn open_steam_valve(&mut self) {
        if !self.interlock.may_open_steam() {
            self.refusals.steam_valve = self.refusals.steam_valve.saturating_add(1);
            warn!(
                "actuators: openSteamValve REFUSED — latched {}, state {:?}, inhibited {} \
                 (the steam whitelist is STEAM_RUNNING and nothing else)",
                self.interlock.latched, self.interlock.state, self.interlock.inhibit.valve
            );
            return;
        }
        // `CLOSED -> STEAM_OPEN`, `WATER_OPEN -> BOTH_OPEN`, and the two
        // already-steam-open states are a no-op. `BOTH_OPEN` is therefore
        // reachable in the type and unreachable in practice, because the two
        // whitelists are disjoint and the reducer consults both.
        self.valve_state = match self.valve_state {
            ValveState::WaterOpen => ValveState::BothOpen,
            ValveState::Closed | ValveState::SteamOpen | ValveState::BothOpen => {
                ValveState::SteamOpen
            }
        };
        self.update_valve_relay();
    }

    fn close_steam_valve(&mut self) {
        self.valve_state = match self.valve_state {
            ValveState::Closed | ValveState::WaterOpen => ValveState::Closed,
            ValveState::SteamOpen | ValveState::BothOpen => ValveState::WaterOpen,
        };
        self.update_valve_relay();
    }

    fn enable_heater(&mut self) {
        // The heater has no separate "enable" pin: the pin belongs to the ISR and
        // the duty is the only thing that moves it, so `enable_heater` is the
        // **permission**, and the gate in [`Self::set_heater_duty`] is what
        // enforces it.
        //
        // The refusal is therefore a *log*, not a state change: there is no cached
        // "heater enabled" flag to keep in step with a pin nobody here owns.
        if self.interlock.may_heat() {
            return;
        }
        warn!(
            "actuators: enableHeater REFUSED — latched {}, inhibited {}",
            self.interlock.latched, self.interlock.inhibit.heater
        );
    }

    fn disable_heater(&mut self) {
        // Always permitted, including while latched: this is the "off" arm, and
        // an interlock that could refuse it would be a trap.
        self.force_heater_duty(0.0);
    }

    fn set_heater_duty(&mut self, duty: f32) {
        if duty > 0.0 && !self.interlock.may_heat() {
            self.refusals.heater = self.refusals.heater.saturating_add(1);
            warn!(
                "actuators: setHeaterDuty({duty}) REFUSED — latched {}, inhibited {}",
                self.interlock.latched, self.interlock.inhibit.heater
            );
            self.force_heater_duty(0.0);
            return;
        }
        // Through `HeaterOutput::set_duty`, always. The deadman gate lives there
        // and nowhere else, so a duty that reaches the heater has been through
        // it. A write that bypassed this would be a safety regression, not an
        // optimisation.
        self.force_heater_duty(duty);
    }

    fn emergency_shutdown(&mut self) {
        // The bookkeeping moves **first**, so a relay write that fails cannot
        // leave the facade believing the machine is not latched.
        self.interlock.latched = true;
        self.valve_state = ValveState::Closed;
        self.pump.set_level(self.pump_polarity.inactive).ok();
        self.valve.set_level(self.valve_polarity.inactive).ok();
        self.force_heater_duty(0.0);
        info!("actuators: EMERGENCY SHUTDOWN — relays off and the latch is set");
    }

    fn safe_hardware_shutdown(&mut self) {
        // The same relays off **without** the latch, so the machine can come back
        // after standby. That is the whole difference between the two methods and
        // the reason there are two.
        self.valve_state = ValveState::Closed;
        self.pump.set_level(self.pump_polarity.inactive).ok();
        self.valve.set_level(self.valve_polarity.inactive).ok();
        self.force_heater_duty(0.0);
        if !self.shutdown_announced {
            info!("actuators: safe hardware shutdown — relays off, latch untouched");
        }
        self.shutdown_announced = true;
    }
}

impl Actuators {
    /// The one place a duty reaches the heater, used by every arm above.
    ///
    /// Taking `&mut self` rather than `&self` is forced by
    /// [`HeaterOutput::set_duty`], which needs `&mut` for the gate.
    fn force_heater_duty(&mut self, duty: f32) {
        // Milliseconds in a 1000 ms window, which is what
        // `cc_machine::Effect::SetHeaterDuty` carries and what
        // `cc_domain::units::Duty` is defined as. A `NaN` is clamped to zero
        // rather than passed on: `duty_counts` would turn it into 0 anyway, but
        // doing it here means the log line and the pin agree.
        let safe = if duty.is_nan() { 0.0 } else { duty };
        // This used to log "the heater duty could not be written … the pin still
        // holds the PREVIOUS duty" on the transport's error, which was the honest
        // reading of a peripheral that refused the write. The `GPTimer` ISR
        // transport writes two atomics and cannot fail, so `set_duty` no longer
        // returns a `Result` and there is no such failure to report.
        self.heater.set_duty(self.now, Duty::new(safe));
    }
}

/// [`cc_machine::MachineChannels`] and [`cc_machine::Diagnostics`] over the
/// firmware's logging, its reboot path and the maintenance counter.
///
/// # Two halves, and both are mandatory
///
/// [`cc_machine::MachineChannels`] has no default bodies anywhere, so this is
/// the place where "the shot counter was never wired up" became impossible.
/// It used to be one fourteen-method trait with a silent `{}` body per method:
/// `on_record_brew` was one of the nine nobody overrode, `machine.shots_since_backflush`
/// stayed at 0, and `/api/status` answered `shotsSinceBackflush: 0` and
/// `backflushReminderDue: false` for the life of the firmware with no compiler
/// error and no failing test. See `cc_machine::applier`'s module documentation.
///
/// [`cc_machine::Diagnostics`] is returned as `Some(self)`, so every
/// observability effect reaches a real sink. The two it has nothing to do about
/// — the MQTT reconnect counter and the display wake — say so in their own
/// documentation there and are empty here on purpose: a missing log line is
/// visible, which is the property that was missing from the shot counter.
pub struct FirmwareSide {
    /// Set by [`Self::on_request_reboot`], read by the control task between
    /// ticks.
    ///
    /// A flag rather than a restart **inside the applier**, because
    /// `Effect::RequestReboot` is emitted in the middle of a tick's effect list
    /// and restarting there would abandon the rest of that list — including a
    /// `CloseWaterValve` further down.
    reboot_requested: bool,
    /// The shot count the applier has been told about and the control task has
    /// not yet written down, taken by [`Self::take_shots_to_persist`].
    ///
    /// The **same deferral as the reboot flag, for the same reason**: the NVS
    /// store is the control task's, and `cc-firmware` uses it inside the tick
    /// for the configuration, the tare and the credentials. A `&mut` borrow of
    /// it held for the life of the applier would make every one of those writes
    /// impossible, so the applier asks and the task answers. It is also what
    /// keeps the write *outside* the effect list: `on_record_brew` runs in the
    /// middle of a tick's effects, and an NVS commit is milliseconds of flash
    /// erase-and-write in the middle of a 10 ms control period.
    ///
    /// `None` means "nothing to write", which is the common case: a brew that
    /// did not qualify, and every effect that is not about the counter.
    shots_to_persist: Option<i32>,
    /// The last value [`Self::take_shots_to_persist`] handed out, so an
    /// unchanged count is not written again.
    ///
    /// Skipping an unchanged value is why a completed backflush is not also a
    /// second erase-and-commit of a key that already says 0. Seeded from the
    /// stored value at boot by [`Self::with_shots_since_backflush`], because
    /// "nothing written yet" and "0 is stored" are different answers to the same
    /// question.
    last_persisted_shots: Option<i32>,
}

impl FirmwareSide {
    /// A side channel that has been asked for nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            reboot_requested: false,
            shots_to_persist: None,
            last_persisted_shots: None,
        }
    }

    /// A side channel that starts from a counter read out of NVS.
    ///
    /// The stored value seeds [`Self::last_persisted_shots`] so that the first
    /// tick does not write a count that is already stored, and so that a reset of
    /// an already-zero counter is recognised as a no-op.
    ///
    /// `None` means nothing was readable, which is a first boot or a partition
    /// this firmware cannot interpret. It is **not** a different behaviour:
    /// [`Self::new`] and this agree that nothing is stored.
    #[must_use]
    pub const fn with_shots_since_backflush(stored: Option<i32>) -> Self {
        Self {
            reboot_requested: false,
            shots_to_persist: None,
            last_persisted_shots: stored,
        }
    }

    /// Whether a reboot was requested, and clears the request.
    ///
    /// One-shot, so a caller that polls it every tick restarts exactly once.
    pub fn take_reboot_request(&mut self) -> bool {
        let asked = self.reboot_requested;
        self.reboot_requested = false;
        asked
    }

    /// The shot count the applier wants written to NVS, and clears the request.
    ///
    /// Taken by the control task, which owns the store, and written with
    /// [`crate::nvs::save_shots_since_backflush`]. `None` means either that
    /// nothing changed or that the value written last is the value asked for
    /// again, and the second is what keeps a completed backflush from
    /// re-committing a key that already says 0.
    pub fn take_shots_to_persist(&mut self) -> Option<i32> {
        let asked = self.shots_to_persist?;
        if self.last_persisted_shots == Some(asked) {
            return None;
        }
        self.shots_to_persist = None;
        Some(asked)
    }

    /// Record that `shots` is what NVS now holds.
    ///
    /// Called by the control task after a **successful** write, so that a
    /// failed one is retried on the next change rather than believed.
    pub fn note_shots_persisted(&mut self, shots: i32) {
        self.last_persisted_shots = Some(shots);
    }
}

impl Default for FirmwareSide {
    fn default() -> Self {
        Self::new()
    }
}

impl MachineChannels for FirmwareSide {
    /// Qualifying and incrementing are the reducer's —
    /// `Machine::shots_since_backflush` has already moved if `counted` — so this
    /// is the write, and it is a flag rather than a write for the reason on
    /// [`FirmwareSide::shots_to_persist`].
    fn on_record_brew(&mut self, counted: bool, shots_since_backflush: i32) {
        if counted {
            info!("maintenance: counted brew, shots since backflush = {shots_since_backflush}");
            self.shots_to_persist = Some(shots_since_backflush);
        } else {
            // The numbers are not on the effect — the reducer has them and made
            // the decision — so this is the count the decision did not move, which
            // is the part an operator asking "why is my machine not counting
            // shots?" wants.
            debug!(
                "maintenance: brew not counted — shots since backflush stays \
                 {shots_since_backflush}"
            );
        }
    }

    /// The reducer has already cleared the counter, so this is the NVS half — the
    /// same half `POST /api/maintenance/reset-backflush-counter` reaches.
    fn on_reset_shots_since_backflush(&mut self, shots_since_backflush: i32) {
        info!("maintenance: reset shots since backflush to {shots_since_backflush}");
        self.shots_to_persist = Some(shots_since_backflush);
    }

    fn on_request_reboot(&mut self) {
        info!("machine: reboot requested — the control task restarts after this tick");
        self.reboot_requested = true;
    }

    fn diagnostics(&mut self) -> Option<&mut dyn Diagnostics> {
        Some(self)
    }
}

impl Diagnostics for FirmwareSide {
    fn on_enter_state(&mut self, state: MachineState) {
        info!("machine: -> {}", state.name());
    }

    fn on_exit_state(&mut self, state: MachineState) {
        info!("machine: <- {}", state.name());
    }

    fn on_pid_runtime(&mut self, enabled: bool) {
        info!("machine: pid runtime {}", if_enabled(enabled));
    }

    fn on_steam_mode(&mut self, enabled: bool) {
        info!("machine: steam mode {}", if_enabled(enabled));
    }

    /// The pump-watchdog trip. `timing.rs` documents `Effect::PumpTimeoutFired`
    /// as the thing that makes a trip "visible in the log", which a field log has
    /// to be able to answer ("did this ever trip?"). Without this,
    /// `Diagnostics::on_log` falls back to the trait's empty body and the trip
    /// disables the pump silently — the same defect class as the
    /// `on_record_beat` silent `{}` that `applier.rs:29-47` records, on the half
    /// of the split where a default body is permitted.
    fn on_log(&mut self, message: &str) {
        warn!("machine: {message}");
    }
}

/// `enabled` / `disabled`, so a log line is not two call sites of `if`.
const fn if_enabled(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub mod tests {
    #![allow(
        clippy::wildcard_imports,
        reason = "the tests exercise the parent's private items, and the `cfg(test)` \
                  exception does not cover the device-tests build"
    )]

    use super::{Polarity, RelayPolarities};
    use cc_domain::hardware::RelayTriggerType;

    #[cfg_attr(test, test)]
    pub fn high_trigger_energises_high() {
        assert_eq!(
            Polarity::high_trigger().active,
            esp_idf_hal::gpio::Level::High
        );
        assert_eq!(
            Polarity::high_trigger().inactive,
            esp_idf_hal::gpio::Level::Low
        );
    }

    #[cfg_attr(test, test)]
    pub fn low_trigger_energises_low() {
        // `Relay::on()`/`off()`: the branch is
        // on the trigger type, and getting it backwards is what made this setting
        // dangerous when it was ignored.
        assert_eq!(
            Polarity::low_trigger().active,
            esp_idf_hal::gpio::Level::Low
        );
        assert_eq!(
            Polarity::low_trigger().inactive,
            esp_idf_hal::gpio::Level::High
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_polarity_follows_the_configured_trigger() {
        assert_eq!(
            Polarity::for_trigger(RelayTriggerType::HighTrigger),
            Polarity::high_trigger()
        );
        assert_eq!(
            Polarity::for_trigger(RelayTriggerType::LowTrigger),
            Polarity::low_trigger()
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_default_bundle_is_high_trigger_throughout() {
        let all = RelayPolarities::all_high_trigger();
        for polarity in [all.pump, all.valve, all.heater] {
            assert_eq!(polarity, Polarity::high_trigger());
        }
    }

    use super::*;

    const RUNNING: Interlock = Interlock {
        latched: false,
        water_tank_full: true,
        state: MachineState::PidNormal,
        inhibit: Inhibit::NONE,
    };

    #[cfg_attr(test, test)]
    pub fn a_latched_machine_may_energise_nothing() {
        // S2 guards the duty as well as the pump and both valves, which is the
        // same rule applied to the one actuator with no enable pin.
        let latched = Interlock {
            latched: true,
            ..RUNNING
        };
        assert!(!latched.may_pump());
        assert!(!latched.may_open_water());
        assert!(!latched.may_open_steam());
        assert!(!latched.may_heat());
    }

    #[cfg_attr(test, test)]
    pub fn an_empty_tank_stops_the_pump_and_the_water_valve_but_not_the_heater() {
        // S4. The boiler is a separate vessel from the reservoir, so an empty
        // tank must not stop the heater unless `keep_heater_on_empty` says so —
        // and that parameter is the *reducer's* gate, not this one.
        let empty = Interlock {
            water_tank_full: false,
            ..RUNNING
        };
        assert!(!empty.may_pump());
        assert!(!empty.may_open_water());
        assert!(empty.may_heat());
    }

    #[cfg_attr(test, test)]
    pub fn the_water_valve_is_gated_on_an_empty_tank() {
        // The water valve is refused as well as the pump, pinned as a test.
        let empty = Interlock {
            water_tank_full: false,
            ..RUNNING
        };
        assert!(!empty.may_open_water());
    }

    #[cfg_attr(test, test)]
    pub fn the_steam_valve_is_whitelist_gated_to_steam_running() {
        // The reducer never emits `OpenSteamValve`, and this is why that is not
        // sufficient on its own: steam and water share one relay.
        for state in cc_domain::state::ALL {
            let probe = Interlock { state, ..RUNNING };
            assert_eq!(
                probe.may_open_steam(),
                state == MachineState::SteamRunning,
                "state {state:?}"
            );
        }
    }

    #[cfg_attr(test, test)]
    pub fn an_inhibit_holds_its_own_actuator_and_nothing_else() {
        // The reason the three flags are separate: a bring-up run holds the water
        // path inhibited with the heater live.
        let inhibited = Interlock {
            inhibit: Inhibit {
                pump: true,
                valve: true,
                heater: false,
            },
            ..RUNNING
        };
        assert!(!inhibited.may_pump());
        assert!(!inhibited.may_open_water());
        assert!(!inhibited.may_open_steam());
        assert!(
            inhibited.may_heat(),
            "the heater must not be caught by a water inhibit"
        );
    }

    #[cfg_attr(test, test)]
    pub fn the_water_valve_is_whitelist_gated_to_the_water_flow_states() {
        // S5, and the mirror of the steam test above. Enforcing it only through
        // the tick's trailing `CloseWaterValve` is one enforcement point too few;
        // see `Interlock::may_open_water`.
        for state in cc_domain::state::ALL {
            let probe = Interlock { state, ..RUNNING };
            assert_eq!(
                probe.may_open_water(),
                cc_safety::water_flow_allowed(state),
                "state {state:?}"
            );
        }
    }

    #[cfg_attr(test, test)]
    pub fn a_healthy_interlock_permits_the_pump_the_valves_and_the_heater() {
        // Both whitelists are the exception even here: `PID_NORMAL` is neither a
        // water state nor a steam state, so both valves stay shut.
        assert!(RUNNING.may_pump());
        assert!(RUNNING.may_heat());
        assert!(!RUNNING.may_open_water());
        assert!(!RUNNING.may_open_steam());
    }

    #[cfg_attr(test, test)]
    pub fn the_valve_relay_is_off_only_when_both_valves_are_closed() {
        // Steam and water share GPIO17, so this is the one place the shared pin's
        // state is decided.
        assert!(!ValveState::Closed.relay_should_be_on());
        assert!(ValveState::SteamOpen.relay_should_be_on());
        assert!(ValveState::WaterOpen.relay_should_be_on());
        assert!(ValveState::BothOpen.relay_should_be_on());
    }

    #[cfg_attr(test, test)]
    pub fn an_empty_tank_at_boot_does_not_block_the_pump() {
        // Assume full. A machine with no float switch fitted must not refuse to
        // pump on its first tick, or it can never brew.
        let boot = Interlock::healthy();
        assert!(boot.may_pump());
        assert!(!Inhibit::NONE.any());
        assert!(Inhibit::ALL.any());
    }
}
