//! The board's sensors, read on their own periods.
//!
//! This is the glue between the drivers and the aggregator, and it is per-board because the pins
//! are. Everything it does is decide *which driver to ask*; the drivers themselves live in their
//! own crates and know nothing about tasks, and the aggregator knows nothing about pins.
//!
//! The temperature sensor is bit-banged 1-Wire, which cannot be done from inside an async task
//! without blocking the executor, so the wiring for it is deliberately absent rather than
//! wrong: a task that bit-bangs a bus for 500 microseconds every 400 milliseconds is a task that
//! stops the control loop doing that, and the C++ firmware's version of this bug is D05. The
//! aggregator accepts the result asynchronously through
//! [`Aggregator::set_temperature`](clevercoffee_app::sensors::Aggregator::set_temperature), which
//! is the shape a completion callback or an interrupt will use.

use clevercoffee_app::machine::Switches;
use clevercoffee_app::sensors::{Aggregator, PressureSource, SwitchSource, TankSource};

/// The board's panel inputs and tank switch, as the aggregator reads them.
#[derive(Debug)]
pub struct BoardInputs<I> {
    inputs: I,
}

impl<I> BoardInputs<I> {
    pub const fn new(inputs: I) -> Self {
        Self { inputs }
    }
}

/// The tank switch, on a board whose switch block exposes it.
impl<I: TankReading> TankSource for BoardInputs<I> {
    fn is_full(&self) -> bool {
        self.inputs.water_tank_full()
    }
}

/// The panel switches.
impl<I: SwitchReading> SwitchSource for BoardInputs<I> {
    fn sample(&self) -> Switches {
        self.inputs.sample()
    }
}

/// What a board's input block must expose for [`BoardInputs`] to be a sensor source.
///
/// A trait rather than a generic over four types, so the aggregator sees one thing and the board
/// implements it once.
pub trait TankReading {
    fn water_tank_full(&self) -> bool;
}

pub trait SwitchReading {
    fn sample(&self) -> Switches;
}

/// A pressure source that is not fitted.
///
/// The C6 and the S3 have a pressure sensor on the same I2C bus as the panel, and a machine
/// without one must not read a pin that is not there. Returning `None` from `read` is the shape
/// the aggregator already handles, so a machine with no pressure sensor simply has no pressure.
#[derive(Debug, Default)]
pub struct NoPressure;

impl PressureSource for NoPressure {
    fn start_conversion(&mut self) {}
    fn read(&mut self) -> Option<f64> {
        None
    }
}

/// Drives the aggregator once per control tick, with a driver's own period deciding the work.
///
/// The whole scheduling decision in one place: three calls, each of which returns immediately if
/// its driver is not due. The machine then takes `aggregator.snapshot()` and never learns that
/// three different clocks exist.
pub fn poll<P, T, S>(
    aggregator: &mut Aggregator,
    now_ms: u32,
    pressure: &mut P,
    tank: &T,
    switches: &S,
) where
    P: PressureSource,
    T: TankSource,
    S: SwitchSource,
{
    aggregator.maybe_step_pressure(now_ms, pressure);
    aggregator.maybe_read_tank(now_ms, tank);
    aggregator.maybe_read_switches(now_ms, switches);
}
