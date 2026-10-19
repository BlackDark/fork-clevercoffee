//! Temperature sensing: the two bus protocols, the two devices, and the one
//! interface `cc-machine` is allowed to see.
//!
//! # Why this is a module and not two more top-level modules
//!
//! Two protocols, two devices, one interface. That is a module, not a crate: a
//! crate boundary here would buy nothing that `mod.rs` does not already give, and
//! the split below is the real one.
//!
//! | file | what it decides | device coupling |
//! | --- | --- | --- |
//! | [`onewire`] | 1-Wire: CRC8, bit order, command set, scratchpad decode, slot timings | none — [`onewire::OneWireBus`] is the seam |
//! | [`ds18b20`] | the DS18B20's non-blocking pipeline and its accept/reject decision | none |
//! | [`tsic306`] | `ZACwire`: strobe acquisition, frame assembly, parity, DS→°C, the range and rate filters | none — [`tsic306::EdgeSource`] is the seam |
//! | [`probe`] | the vocabulary both drivers report in (`ProbeReading`, `ProbeFault`, `ProbeSource`) | none |
//!
//! Every protocol decision is here and host-tested; `cc-hal-esp32` supplies a
//! pin and a clock and nothing else. That is the whole reason the protocol
//! logic is portable: neither the 1-Wire bus nor the `ZACwire` waveform can be
//! exercised on a host without it, and a bit-bang protocol that has only ever
//! been run on a machine with a sensor soldered to it is a bit-bang protocol
//! nobody can review.
//!
//! # The one interface
//!
//! The three types in [`probe`] plus two free functions, [`ds18b20::as_probe`] and
//! [`tsic306::as_probe`], each of which collapses one of its driver's outcomes onto
//! [`ProbeReading`] or `None`. Neither driver type leaks past them, so a machine with a DS18B20 and
//! a machine with a TSIC-306 run the same reducer. There is deliberately no trait and no `dyn`
//! probe: a trait here would have to be implemented once per driver to say what those two functions
//! already say.
//!
//! # The two drivers do not share a range check
//!
//! They disagree on purpose, and each module says why:
//!
//! * [`tsic306`] rejects a reading at or below `0.0` and at or above `180.0`. That window is what
//!   keeps its two fault sentinels -- 221 and 222 -- out of the temperature stream.
//! * [`ds18b20`] rejects a reading outside `-50.0 ..= 150.0`, which is the range the DS18B20 itself
//!   can produce.
//!
//! The TSIC window is the narrower one and it costs a real reading: a TSIC probe at 0.00 °C is
//! refused. Making them agree would change whichever one is changed, and the narrower one fails in
//! the safe direction. [`probe::ProbeReading::is_usable`] is the shared *query*; neither driver
//! filters on it.

pub mod ds18b20;
pub mod hx711;
pub mod onewire;
pub mod probe;
pub mod tsic306;

pub use probe::{ProbeFault, ProbeReading, ProbeSource};
