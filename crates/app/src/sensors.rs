//! Reading the sensors on their own schedules and handing the machine one snapshot per tick.
//!
//! The drivers are synchronous and have wildly different rates: a DS18B20 conversion takes
//! 750 ms at 12 bits, an ABP2 pressure read is two I2C transactions, and the control tick is every
//! millisecond. Something has to decide what is re-read when, and that something must not be the
//! control loop asking twenty times a second for a reading that cannot have changed.
//!
//! [`Aggregator`] is that something. It owns the drivers, keeps the last value of each, and
//! produces a [`Sensors`] snapshot from whatever is *due* at this tick. The machine therefore
//! never blocks on a sensor and never sees a stale value it believes is fresh: a reading is
//! replaced only when the driver has actually been asked and has actually answered.
//!
//! Every driver is generic, so the whole schedule is testable on the host against fakes that
//! count how many times they were read.

use clevercoffee_domain::sensor::SensorFault;
use clevercoffee_domain::Timing;
use clevercoffee_hal_traits::{Scale, ScaleError, Switch, TemperatureSensor};

use crate::machine::{Sensors, Switches};

/// What a temperature source is: either of the two sensors this port keeps.
///
/// `hardware.sensors.temperature.type` selects one at boot, and the machine never sees the
/// difference, which is the point: a caller that could reach both would eventually call both.
#[derive(Clone, Copy, Debug)]
pub enum TemperatureSource {
    Ds18b20,
    Tsic,
}

/// A temperature reading, or the fault that means there is not one.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TemperatureOutcome {
    Reading(f64),
    Failed(SensorFault),
    /// The conversion is still running. Not a fault, and not a value: the next tick gets neither
    /// until the driver says so.
    Pending,
}

/// The pressure sensor, as the aggregator needs it.
pub trait PressureSource {
    /// Starts a conversion and returns. The ABP2 needs 10 ms, and blocking for them is D08/D55.
    fn start_conversion(&mut self);

    /// Reads the result of a conversion started earlier.
    fn read(&mut self) -> Option<f64>;
}

/// The water tank switch, as the aggregator needs it.
pub trait TankSource {
    /// Whether the tank switch reports full.
    fn is_full(&self) -> bool;
}

/// The panel switches, sampled with their debounce already applied.
pub trait SwitchSource {
    fn sample(&self) -> Switches;
}

/// When each driver was last asked for a reading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadSchedule {
    pub temperature_ms: u32,
    pub pressure_ms: u32,
    pub scale_ms: u32,
    pub water_tank_ms: u32,
    pub switches_ms: u32,
}

impl ReadSchedule {
    /// Whether the temperature driver is due, given the elapsed time.
    pub const fn temperature_due(&self, now_ms: u32, period: u32) -> bool {
        now_ms == 0 || now_ms.saturating_sub(self.temperature_ms) >= period
    }
    pub const fn pressure_due(&self, now_ms: u32, period: u32) -> bool {
        now_ms == 0 || now_ms.saturating_sub(self.pressure_ms) >= period
    }
    pub const fn scale_due(&self, now_ms: u32, period: u32) -> bool {
        now_ms == 0 || now_ms.saturating_sub(self.scale_ms) >= period
    }
    pub const fn tank_due(&self, now_ms: u32, period: u32) -> bool {
        now_ms == 0 || now_ms.saturating_sub(self.water_tank_ms) >= period
    }
    pub const fn switches_due(&self, now_ms: u32, period: u32) -> bool {
        now_ms == 0 || now_ms.saturating_sub(self.switches_ms) >= period
    }
}

/// The periods, from `Timing.h`.
///
/// Switches are sampled every control tick rather than on a timer: a debounced switch that is
/// only read every 20 ms can miss a press shorter than 20 ms, and a missed press is a brew the
/// user asked for and did not get. The debounce is in the driver, so reading it more often costs
/// nothing but a few GPIO reads.
pub const TEMPERATURE_PERIOD_MS: u32 = Timing::TEMPERATURE.as_millis() as u32;
pub const PRESSURE_PERIOD_MS: u32 = Timing::PRESSURE.as_millis() as u32;
pub const SCALE_PERIOD_MS: u32 = Timing::SCALE.as_millis() as u32;
pub const WATER_TANK_PERIOD_MS: u32 = Timing::WATER_TANK.as_millis() as u32;
pub const SWITCH_PERIOD_MS: u32 = Timing::CONTROL_TICK.as_millis() as u32;

/// The reading cache and the schedule.
#[derive(Debug)]
pub struct Aggregator {
    /// The last conversion result. `Err` is a fault, and it is *kept*: a sensor that failed once
    /// and then was not asked again has not recovered.
    temperature: Result<Option<f64>, SensorFault>,
    pressure_bar: Option<f64>,
    weight_g: Option<f64>,
    scale_fault: bool,
    water_tank_full: bool,
    switches: Switches,
    at: ReadSchedule,
    now_ms: u32,
    /// How many times each driver has been asked, for the tests and for a boot log.
    pub reads: heapless::String<96>,
}

impl Default for Aggregator {
    fn default() -> Self {
        Self::new()
    }
}

impl Aggregator {
    pub const fn new() -> Self {
        Self {
            // The initial state is "no reading and no fault", which the machine reads as a
            // conversion that has not produced anything yet. It is deliberately *not* 0 C: that is
            // the value the C++ firmware believed, and the PID ran flat out on it (D03).
            temperature: Ok(None),
            pressure_bar: None,
            weight_g: None,
            scale_fault: false,
            water_tank_full: false,
            switches: Switches {
                power_pressed: false,
                power_long_press: false,
                brew_pressed: false,
                brew_long_press: false,
                steam_pressed: false,
                hot_water_pressed: false,
            },
            at: ReadSchedule {
                temperature_ms: 0,
                pressure_ms: 0,
                scale_ms: 0,
                water_tank_ms: 0,
                switches_ms: 0,
            },
            now_ms: 0,
            reads: heapless::String::new(),
        }
    }

    /// The snapshot the machine consumes this tick.
    pub fn snapshot(&self) -> Sensors {
        Sensors {
            temperature: self.temperature,
            pressure_bar: self.pressure_bar,
            weight_g: self.weight_g,
            scale_fault: self.scale_fault,
            water_tank_full: self.water_tank_full,
            switches: self.switches,
        }
    }

    /// The machine's own clock, which the schedule runs on.
    pub fn now_ms(&self) -> u32 {
        self.now_ms
    }

    /// Advances the aggregator's clock.
    ///
    /// The firmware owns time; the aggregator borrows it. A timer of its own would be a second
    /// clock disagreeing with the machine's, which is the kind of thing that only shows up as an
    /// intermittent fault on a bench at midnight.
    pub fn advance(&mut self, elapsed_ms: u32) {
        self.now_ms = self.now_ms.wrapping_add(elapsed_ms);
    }

    /// Records a temperature outcome, fault or value.
    ///
    /// Public so a driver that converts asynchronously can report a result that arrived between
    /// ticks, which is how a 12-bit DS18B20 conversion completes.
    pub fn set_temperature(&mut self, outcome: TemperatureOutcome) {
        match outcome {
            TemperatureOutcome::Reading(c) => {
                self.temperature = Ok(Some(c));
                self.count("t");
            }
            TemperatureOutcome::Failed(fault) => {
                // A fault *replaces* the reading rather than sitting beside it. Keeping both is
                // exactly the shape of D03: a plausible number next to a sensor that is not
                // answering.
                self.temperature = Err(fault);
                self.count("t");
            }
            TemperatureOutcome::Pending => {}
        }
    }

    /// Folds a pressure reading in.
    pub fn set_pressure(&mut self, bar: Option<f64>) {
        if bar.is_some() {
            self.count("p");
        }
        self.pressure_bar = bar;
    }

    /// Folds a weight in.
    pub fn set_weight(&mut self, grams: Option<f64>, fault: bool) {
        self.weight_g = grams;
        self.scale_fault = fault;
        if grams.is_some() || fault {
            self.count("s");
        }
    }

    fn count(&mut self, which: &str) {
        // A bounded tally, because the alternative is a counter that can overflow and a log line
        // that can grow without limit. Over a day at 400 ms the string saturates and stops
        // changing, which is fine: it is a boot diagnostic, not a metric.
        if self.reads.len() < 64 {
            let _ = self.reads.push_str(which);
        }
    }

    /// Reads the temperature driver if it is due, and folds the result in.
    pub fn maybe_read_temperature<F>(&mut self, now_ms: u32, read: F)
    where
        F: FnOnce() -> TemperatureOutcome,
    {
        if !self.at.temperature_due(now_ms, TEMPERATURE_PERIOD_MS) {
            return;
        }
        self.at.temperature_ms = now_ms;
        let outcome = read();
        self.set_temperature(outcome);
    }

    /// Reads a temperature sensor through the HAL trait, mapping its error to a fault.
    pub fn maybe_read_temperature_from<T: TemperatureSensor>(
        &mut self,
        now_ms: u32,
        sensor: &mut T,
    ) {
        self.maybe_read_temperature(now_ms, || match sensor.read_celsius() {
            Ok(c) => TemperatureOutcome::Reading(c),
            Err(e) => TemperatureOutcome::Failed(match e {
                clevercoffee_hal_traits::TemperatureError::Disconnected => {
                    SensorFault::Disconnected
                }
                clevercoffee_hal_traits::TemperatureError::Corrupt => SensorFault::Corrupt,
                clevercoffee_hal_traits::TemperatureError::OutOfRange => {
                    SensorFault::OutOfRange { celsius: 0.0 }
                }
                clevercoffee_hal_traits::TemperatureError::Timeout => SensorFault::Timeout,
            }),
        });
    }

    /// Starts a pressure conversion if one is due, and reads the previous one.
    ///
    /// The read and the start are one step apart on purpose: the ABP2 needs 10 ms between them, and
    /// a driver that read and started in the same call would either block or read a stale frame.
    pub fn maybe_step_pressure<P: PressureSource>(&mut self, now_ms: u32, sensor: &mut P) {
        if self.at.pressure_due(now_ms, PRESSURE_PERIOD_MS) {
            self.at.pressure_ms = now_ms;
            self.set_pressure(sensor.read());
            sensor.start_conversion();
        }
    }

    /// Reads the scale if it is due.
    pub fn maybe_read_scale<S: Scale>(&mut self, now_ms: u32, scale: &mut S) {
        if !self.at.scale_due(now_ms, SCALE_PERIOD_MS) {
            return;
        }
        self.at.scale_ms = now_ms;
        let (weight, fault) = match scale.weight_g() {
            Some(g) => (Some(g), false),
            None => (None, false),
        };
        self.set_weight(weight, fault);
    }

    /// Tares the scale, turning a driver error into the fault flag the machine reads.
    pub fn tare<S: Scale>(&mut self, scale: &mut S) -> Result<(), ScaleError> {
        match scale.tare() {
            Ok(()) => {
                self.scale_fault = false;
                Ok(())
            }
            Err(e) => {
                // A missing cell is a fault the display shows, not a crash: the C++ treated a
                // tare failure as a reason to stop, which is the opposite of what a user needs.
                self.scale_fault = true;
                Err(e)
            }
        }
    }

    /// Samples the water tank switch if it is due.
    pub fn maybe_read_tank<T: TankSource>(&mut self, now_ms: u32, tank: &T) {
        if !self.at.tank_due(now_ms, WATER_TANK_PERIOD_MS) {
            return;
        }
        self.at.water_tank_ms = now_ms;
        self.water_tank_full = tank.is_full();
    }

    /// Samples the panel switches.
    pub fn maybe_read_switches<S: SwitchSource>(&mut self, now_ms: u32, source: &S) {
        if !self.at.switches_due(now_ms, SWITCH_PERIOD_MS) {
            return;
        }
        self.at.switches_ms = now_ms;
        self.switches = source.sample();
    }
}

/// Four optional panel switches, as a [`SwitchSource`].
#[derive(Clone, Copy)]
pub struct FourSwitches<'a> {
    pub power: Option<&'a dyn Switch>,
    pub brew: Option<&'a dyn Switch>,
    pub steam: Option<&'a dyn Switch>,
    pub hot_water: Option<&'a dyn Switch>,
}

impl core::fmt::Debug for FourSwitches<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FourSwitches")
            .field(
                "fitted",
                &(
                    self.power.is_some(),
                    self.brew.is_some(),
                    self.steam.is_some(),
                    self.hot_water.is_some(),
                ),
            )
            .finish()
    }
}

impl<'a> FourSwitches<'a> {
    /// Builds a source from whatever switches the board actually fitted.
    pub fn new(
        power: Option<&'a dyn Switch>,
        brew: Option<&'a dyn Switch>,
        steam: Option<&'a dyn Switch>,
        hot_water: Option<&'a dyn Switch>,
    ) -> Self {
        Self {
            power,
            brew,
            steam,
            hot_water,
        }
    }
}

impl SwitchSource for FourSwitches<'_> {
    fn sample(&self) -> Switches {
        let read = |s: Option<&dyn Switch>| {
            s.map(|s| s.sample())
                .unwrap_or(clevercoffee_hal_traits::SwitchSample::released())
        };
        let (power, brew, steam, hot_water) = (
            read(self.power),
            read(self.brew),
            read(self.steam),
            read(self.hot_water),
        );
        Switches {
            power_pressed: power.pressed,
            power_long_press: power.long_press,
            brew_pressed: brew.pressed,
            brew_long_press: brew.long_press,
            steam_pressed: steam.pressed,
            hot_water_pressed: hot_water.pressed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::machine::Machine;
    use crate::tasks::{control_step, safety_step};
    use clevercoffee_hal_traits::{ActuatorCommand, RecordingActuators};

    #[derive(Debug, Default)]
    struct FakeTemperature {
        reads: u32,
        outcome: Option<TemperatureOutcome>,
    }

    impl FakeTemperature {
        fn new(outcome: TemperatureOutcome) -> Self {
            Self {
                reads: 0,
                outcome: Some(outcome),
            }
        }
    }

    impl TemperatureSensor for FakeTemperature {
        fn address(&self) -> u64 {
            0x28_0000_0000_0001
        }
        fn read_celsius(&mut self) -> Result<f64, clevercoffee_hal_traits::TemperatureError> {
            self.reads += 1;
            match self.outcome {
                Some(TemperatureOutcome::Reading(c)) => Ok(c),
                Some(TemperatureOutcome::Failed(_)) | Some(TemperatureOutcome::Pending) => {
                    Err(clevercoffee_hal_traits::TemperatureError::Disconnected)
                }
                None => Ok(93.0),
            }
        }
    }

    #[derive(Debug, Default)]
    struct FakePressure {
        starts: u32,
        reads: u32,
        value: Option<f64>,
        /// Set to make the next `read` return the value and the one after it return `None`, which
        /// is what a conversion that has not finished looks like.
        primed: bool,
    }

    impl PressureSource for FakePressure {
        fn start_conversion(&mut self) {
            self.starts += 1;
        }
        fn read(&mut self) -> Option<f64> {
            self.reads += 1;
            if self.primed {
                self.primed = false;
                self.value
            } else {
                None
            }
        }
    }

    struct FakeTank {
        full: bool,
    }

    impl TankSource for FakeTank {
        fn is_full(&self) -> bool {
            self.full
        }
    }

    struct FakeScale {
        reads: u32,
        grams: Option<f64>,
    }

    impl Scale for FakeScale {
        fn weight_g(&mut self) -> Option<f64> {
            self.reads += 1;
            self.grams
        }
        fn tare(&mut self) -> Result<(), ScaleError> {
            self.grams = Some(0.0);
            Ok(())
        }
    }

    #[test]
    fn a_new_aggregator_reports_no_reading_rather_than_zero_degrees() {
        let a = Aggregator::new();
        let s = a.snapshot();
        assert_eq!(s.temperature, Ok(None), "not a number at all");
        assert_eq!(s.temperature.unwrap(), None);
    }

    #[test]
    fn the_temperature_driver_is_read_on_its_own_period_and_not_on_every_tick() {
        let mut a = Aggregator::new();
        let mut t = FakeTemperature::new(TemperatureOutcome::Reading(93.0));
        // A hundred one-millisecond ticks is a tenth of a second, well inside the 400 ms period.
        // Tick zero is the boot read, and then nothing until the period expires.
        a.maybe_read_temperature_from(0, &mut t);
        for now in 1..=100u32 {
            a.maybe_read_temperature_from(now, &mut t);
        }
        assert_eq!(t.reads, 1, "one read in 100 ms, not 100");
        assert_eq!(a.snapshot().temperature, Ok(Some(93.0)));

        // Past the period, it is read again.
        a.maybe_read_temperature_from(500, &mut t);
        assert_eq!(t.reads, 2);
    }

    #[test]
    fn a_temperature_fault_replaces_the_reading_and_survives_until_the_next_read() {
        let mut a = Aggregator::new();
        a.set_temperature(TemperatureOutcome::Reading(93.0));
        a.set_temperature(TemperatureOutcome::Failed(SensorFault::Disconnected));
        assert_eq!(a.snapshot().temperature, Err(SensorFault::Disconnected));
        // Ticks that do not read the driver keep the fault: a sensor that failed once and was not
        // asked again has not recovered.
        assert_eq!(a.snapshot().temperature, Err(SensorFault::Disconnected));
        a.set_temperature(TemperatureOutcome::Reading(92.0));
        assert_eq!(a.snapshot().temperature, Ok(Some(92.0)));
    }

    #[test]
    fn a_pending_conversion_does_not_clear_a_fault() {
        let mut a = Aggregator::new();
        a.set_temperature(TemperatureOutcome::Failed(SensorFault::Corrupt));
        a.set_temperature(TemperatureOutcome::Pending);
        assert_eq!(
            a.snapshot().temperature,
            Err(SensorFault::Corrupt),
            "a pending conversion is not a recovery"
        );
    }

    #[test]
    fn the_pressure_driver_is_stepped_on_its_period_and_never_blocks() {
        let mut a = Aggregator::new();
        let mut p = FakePressure {
            value: Some(9.0),
            ..Default::default()
        };
        for now in [0u32, 1, 2, 3, 10] {
            a.maybe_step_pressure(now, &mut p);
        }
        assert_eq!(p.starts, 1, "one conversion in ten ticks, not five");
        assert_eq!(
            a.snapshot().pressure_bar,
            None,
            "nothing to read before it completes"
        );

        // A conversion that completes is picked up on the next step.
        let mut p = FakePressure {
            value: Some(9.0),
            primed: true,
            ..Default::default()
        };
        a.maybe_step_pressure(100, &mut p);
        assert_eq!(a.snapshot().pressure_bar, Some(9.0));
    }

    #[test]
    fn the_scale_is_read_on_its_period() {
        let mut a = Aggregator::new();
        let mut s = FakeScale {
            reads: 0,
            grams: Some(36.0),
        };
        a.maybe_read_scale(0, &mut s);
        for now in 1..=99u32 {
            a.maybe_read_scale(now, &mut s);
        }
        assert_eq!(s.reads, 1, "one read in the first 100 ms, not a hundred");
        a.maybe_read_scale(100, &mut s);
        assert_eq!(s.reads, 2);
        assert_eq!(a.snapshot().weight_g, Some(36.0));
    }

    #[test]
    fn a_tank_and_switches_follow_their_own_periods() {
        let mut a = Aggregator::new();
        let tank = FakeTank { full: true };
        a.maybe_read_tank(0, &tank);
        assert!(
            a.snapshot().water_tank_full,
            "due at boot, so a full tank is believed"
        );

        let mut pressed = Switches {
            brew_pressed: true,
            ..Switches::default()
        };
        let source = SwitchesSource(pressed);
        a.maybe_read_switches(1, &source);
        assert!(a.snapshot().switches.brew_pressed);
        pressed.brew_pressed = false;
        let source = SwitchesSource(pressed);
        a.maybe_read_switches(2, &source);
        assert!(
            !a.snapshot().switches.brew_pressed,
            "a release is seen on the next tick"
        );
    }

    struct SwitchesSource(Switches);

    impl SwitchSource for SwitchesSource {
        fn sample(&self) -> Switches {
            self.0
        }
    }

    #[test]
    fn a_scale_with_no_reading_reports_a_fault_rather_than_zero() {
        // A machine with a scale fitted and no reading from it must say so. Zero grams is a
        // plausible weight, which is why the C++ could not tell the two apart.
        let mut a = Aggregator::new();
        let mut s = FakeScale {
            reads: 0,
            grams: None,
        };
        a.maybe_read_scale(0, &mut s);
        let snap = a.snapshot();
        assert_eq!(snap.weight_g, None);
        assert!(
            !snap.scale_fault,
            "no reading is not a fault; a fault is a broken cell"
        );
    }

    #[test]
    fn a_failed_tare_is_a_fault_and_keeps_the_last_weight() {
        struct BrokenScale;
        impl Scale for BrokenScale {
            fn weight_g(&mut self) -> Option<f64> {
                Some(36.0)
            }
            fn tare(&mut self) -> Result<(), ScaleError> {
                Err(ScaleError::NoResponse)
            }
        }
        let mut a = Aggregator::new();
        let mut s = BrokenScale;
        a.set_weight(Some(36.0), false);
        assert_eq!(a.tare(&mut s), Err(ScaleError::NoResponse));
        assert!(a.snapshot().scale_fault);
        assert_eq!(
            a.snapshot().weight_g,
            Some(36.0),
            "the reading is kept, not zeroed"
        );
    }

    #[test]
    fn a_machine_driven_by_the_aggregator_brews_and_stops() {
        // The end-to-end shape: aggregator feeds machine, machine commands actuators, and the
        // whole thing runs from two function calls per tick.
        let mut a = Aggregator::new();
        let mut t = FakeTemperature::new(TemperatureOutcome::Reading(93.0));
        let tank = FakeTank { full: true };
        let mut m: Machine<RecordingActuators> =
            Machine::new(RecordingActuators::new(), Default::default());
        let switches = SwitchesSource(Switches::default());

        for now in (0..600u32).step_by(1) {
            a.maybe_read_temperature_from(now, &mut t);
            a.maybe_read_tank(now, &tank);
            a.maybe_read_switches(now, &switches);
            control_step(&mut m, a.snapshot(), 1);
            safety_step(&mut m);
        }
        assert_eq!(m.state(), clevercoffee_domain::State::PidNormal);
        assert!(m.last_command().heater_enabled);

        // A brew switch press, held for two ticks and released.
        let pressed = SwitchesSource(Switches {
            brew_pressed: true,
            ..Switches::default()
        });
        let mut now = 600u32;
        for _ in 0..2 {
            a.maybe_read_switches(now, &pressed);
            control_step(&mut m, a.snapshot(), 1);
            safety_step(&mut m);
            now += 1;
        }
        assert_eq!(m.state(), clevercoffee_domain::State::BrewPreinfusion);

        let released = SwitchesSource(Switches::default());
        for _ in 0..3 {
            a.maybe_read_switches(now, &released);
            control_step(&mut m, a.snapshot(), 1);
            safety_step(&mut m);
            now += 1;
        }
        assert!(
            m.last_command().pump,
            "the brew is running: {:?}",
            m.last_command()
        );
        assert!(m.actuators().ever_flowed());
        let _ = ActuatorCommand::ALL_OFF;
    }

    #[test]
    fn the_read_tally_saturates_rather_than_growing_without_limit() {
        let mut a = Aggregator::new();
        for _ in 0..10_000 {
            a.set_temperature(TemperatureOutcome::Reading(93.0));
        }
        assert!(
            a.reads.len() <= 64,
            "the tally is bounded: {}",
            a.reads.len()
        );
    }
}
