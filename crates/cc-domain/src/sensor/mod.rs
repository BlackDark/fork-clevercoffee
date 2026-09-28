//! Temperature sensing: the two bus protocols, the two devices, and the one
//! interface `cc-machine` is allowed to see.
//!
//! Owner: **R1-03** (DS18B20) and **R3-07** (TSIC-306 / `ZACwire`).
//!
//! # Why this is a module and not two more top-level modules
//!
//! The C++ puts both drivers in one directory
//! (`src/hardware/tempsensors/{TempSensorDallas,TempSensorTSIC}.cpp`) behind one
//! base class (`TempSensor`), and the *only* thing that base class adds over the
//! two drivers is a bad-reading counter, a moving average and a range check
//! nobody calls. Two protocols, two devices, one interface — that is a module,
//! not a crate, because a crate boundary here would buy nothing that
//! `mod.rs` does not already give: the split below is the real one.
//!
//! | file | what it decides | device coupling |
//! | --- | --- | --- |
//! | [`onewire`] | 1-Wire: CRC8, bit order, command set, scratchpad decode, slot timings | none — [`onewire::OneWireBus`] is the seam |
//! | [`ds18b20`] | the DS18B20's non-blocking pipeline and its accept/reject decision | none |
//! | [`tsic306`] | `ZACwire`: strobe acquisition, frame assembly, parity, DS→°C, the C++'s safety filters | none — [`tsic306::EdgeSource`] is the seam |
//! | [`probe`] | the one interface both expose | none |
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
//! [`TemperatureProbe`] exists so that neither [`ds18b20::Driver`] nor
//! [`tsic306::Tsic306`] leaks into the state machine. The C++ gets this for
//! free from `TempSensor` being a base class; Rust has no inheritance, so the
//! same shape is a trait. `cc-machine` takes a `&mut dyn TemperatureProbe` and
//! cannot tell which sensor is on the other end, which is the point — a
//! machine with a DS18B20 and a machine with a TSIC-306 run the same reducer.
//!
//! # The two safety-relevant differences between the drivers are *not* levelled
//!
//! The two C++ drivers do not agree about what a valid reading is, and the port
//! keeps the disagreement ([`tsic306`] and [`ds18b20`] each say why):
//!
//! * `TempSensorTSIC` rejects `temp <= 0.0 || temp >= 180.0` outright
//!   (`TempSensorTSIC.cpp:59-62`).
//! * `TempSensorDallas` has no range check at all — `TempSensor::isValidTemperature`
//!   is dead code (09 §18) — so a 165 °C reading is cached, averaged and handed
//!   to the PID.
//!
//! Making them agree would be a behaviour change to whichever one is changed,
//! and the one that is "wrong" (Dallas) fails towards S1 latching, which is the
//! safe direction. [`probe::ProbeReading::is_usable`] is the shared *query*;
//! neither driver filters on it.

pub mod ds18b20;
pub mod onewire;
pub mod probe;
pub mod tsic306;

pub use probe::{ProbeFault, ProbeReading, ProbeSource, TemperatureProbe};
