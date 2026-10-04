//! The incremental, time-budgeted MQTT publish plan.
//!
//! Owner: **R3-13** (task D).
//!
//! # The design being ported
//!
//! `MQTTManager::writeSysParamsToMQTT` (`src/network/MQTTManager.cpp:366-570`)
//! publishes three collections of topics — parameters, numeric sensors, binary
//! sensors — and it publishes them **a few per call**, resuming where it left
//! off on the next call, under a **10 ms budget per call**
//! (`MQTTManager.h:259` `timeBudget_ = 10`).
//!
//! That budget is the interesting part and it is not an optimisation, it is a
//! safety property. A machine with ~50 registered MQTT topics, each publish a
//! TLS-less TCP round trip on a lossy link, can spend a second or more in one
//! pass. The C++ calls `writeSysParamsToMQTT` from the main loop
//! (`LoopManager.cpp:505`), so a second in one pass is a second during which the
//! control loop does not run: the heater is not re-evaluated and the overtemp
//! debounce does not advance. Ten milliseconds is 2.5 % of the 400 ms sensor
//! interval, which is a bound the control loop can absorb.
//!
//! # What is *not* ported, and why
//!
//! The C++'s cursors are `std::map` iterators held as members
//! (`mqttVarsIt_`, `mqttSensorsIt_`, `mqttBinarySensorsIt_`, plus
//! `publishPhase_`), and `std::map` iterators are invalidated by any insertion.
//! Registration happens at startup, so it works, but it means the object cannot
//! be reasoned about: there is no way to ask "how far through phase 1 am I?"
//! without reading three private members.
//!
//! Here a cursor is **three `usize` indices and a phase**, over three slices the
//! caller owns. Which means the whole thing is a host test: the invariant worth
//! pinning is that resuming after a budget expiry picks up at exactly the item
//! that was not published, and that a full pass visits every item exactly once
//! and returns to the start.

/// The three collections the C++ publishes, in the order it publishes them.
///
/// The order is not arbitrary: parameters are retained and change rarely,
/// numeric sensors are polled and change constantly, and binary sensors are
/// retained `ON`/`OFF` strings that may not change for days. Publishing
/// cheapest-and-most-volatile last means a budget expiry drops the *least*
/// important work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    /// `mqttVars_` — the retained parameter topics (`MQTTManager.cpp:410-497`).
    #[default]
    Parameters,
    /// `mqttSensors_` — the polled numeric sensors (`:500-517`).
    Sensors,
    /// `mqttBinarySensors_` — the retained `ON`/`OFF` topics (`:520-548`).
    BinarySensors,
}

impl Phase {
    /// Every phase, in publication order.
    pub const ALL: [Self; 3] = [Self::Parameters, Self::Sensors, Self::BinarySensors];

    /// The C++'s `publishPhase_` integer, for a log line.
    #[must_use]
    pub const fn as_index(self) -> usize {
        match self {
            Self::Parameters => 0,
            Self::Sensors => 1,
            Self::BinarySensors => 2,
        }
    }
}

/// A topic to publish, and how to retain it.
///
/// The C++'s two `publish()` overloads differ in one respect that matters: the
/// parameter and binary-sensor paths pass `retain = true`
/// (`MQTTManager.cpp:492, 545`) and the numeric-sensor path does not
/// (`:511`). Retention is not cosmetic — a retained `ON` for a contactor is what
/// makes Home Assistant show the right thing after a broker restart, and the C++
/// says so at `:543-544` ("binary state must survive broker/HA restarts").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item<'a> {
    /// The full topic, prefix and hostname already applied.
    pub topic: &'a str,
    /// Whether to set the MQTT retain flag.
    pub retain: bool,
}

/// Where a pass through the three collections has got to.
///
/// A cursor with a phase and three indices. It is `Copy`, 32 bytes, and holds
/// no reference to the collections, so the caller can rebuild them (from a
/// configuration change, say) without invalidating it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    phase: Phase,
    index: usize,
    /// How many items have been offered since the cursor was created or was
    /// last reset.
    ///
    /// Counted, not derived, because the useful question is not "where am I"
    /// but "how much of this pass is done" — and after a budget expiry that
    /// number is what tells a log line whether the machine is keeping up.
    offered: u32,
}

impl Cursor {
    /// A cursor at the start of the parameter phase.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            phase: Phase::Parameters,
            index: 0,
            offered: 0,
        }
    }

    /// The phase currently being published.
    #[must_use]
    pub const fn phase(&self) -> Phase {
        self.phase
    }

    /// The index within that phase.
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }
}

/// The three collections, as the caller built them.
///
/// A `&[Item]` per phase rather than a `Vec`, so the caller owns the storage
/// and the cursor can be `Copy`. On the device these are `heapless` or `alloc`
/// vectors built once at startup from the MQTT topic registrations.
#[derive(Clone, Copy, Debug)]
pub struct Plan<'a> {
    /// Retained parameter topics.
    pub parameters: &'a [Item<'a>],
    /// Non-retained polled sensors.
    pub sensors: &'a [Item<'a>],
    /// Retained binary sensors.
    pub binary_sensors: &'a [Item<'a>],
}

impl<'a> Plan<'a> {
    /// The slice for a phase.
    #[must_use]
    pub const fn phase_items(&self, phase: Phase) -> &'a [Item<'a>] {
        match phase {
            Phase::Parameters => self.parameters,
            Phase::Sensors => self.sensors,
            Phase::BinarySensors => self.binary_sensors,
        }
    }

    /// Total items across all three phases.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.parameters.len() + self.sensors.len() + self.binary_sensors.len()
    }

    /// Whether there is nothing to publish.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The next item to publish, advancing the cursor.
    ///
    /// `None` when the cursor has just wrapped past the end, i.e. one complete
    /// pass has been offered. The caller then stops, having satisfied the time
    /// budget, and calls again on the next iteration to start the next pass.
    ///
    /// Phases with no items are skipped rather than stalling, so a machine with
    /// no binary sensors does not spend one iteration per empty phase.
    pub const fn next(&self, cursor: &mut Cursor) -> Option<Item<'a>> {
        let mut phase = cursor.phase;
        let mut index = cursor.index;
        // At most two full laps: one to leave the current phase and one to
        // prove the end of the last phase. Bounded so this cannot spin on a
        // plan whose slices are all empty.
        let mut remaining = 2;
        while remaining > 0 {
            let items = self.phase_items(phase);
            if index < items.len() {
                let item = items[index];
                index += 1;
                cursor.phase = phase;
                cursor.index = index;
                cursor.offered += 1;
                return Some(item);
            }
            // This phase is done or empty; move on. `as_index() + 1 == 3` wraps
            // to `Parameters`, which is the wrap the C++ does at
            // `MQTTManager.cpp:571-573`.
            let next = (phase.as_index() + 1) % 3;
            if next == 0 {
                // Wrapped: the pass is complete.
                cursor.phase = Phase::Parameters;
                cursor.index = 0;
                return None;
            }
            // `next` is 1 or 2 here; the wrap was handled above.
            phase = match next {
                1 => Phase::Sensors,
                _ => Phase::BinarySensors,
            };
            index = 0;
            remaining -= 1;
        }
        None
    }
}

impl Cursor {
    /// How many items this cursor has offered since it was created.
    #[must_use]
    pub const fn offered(&self) -> u32 {
        self.offered
    }

    /// Forget the progress and start again at the first parameter.
    pub const fn reset(&mut self) {
        self.phase = Phase::Parameters;
        self.index = 0;
        self.offered = 0;
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;

    const PARAMS: [Item; 3] = [
        Item {
            topic: "cc/kitchen/brewSetpoint",
            retain: true,
        },
        Item {
            topic: "cc/kitchen/steamSetpoint",
            retain: true,
        },
        Item {
            topic: "cc/kitchen/pidON",
            retain: true,
        },
    ];
    const SENSORS: [Item; 2] = [
        Item {
            topic: "cc/kitchen/temperature",
            retain: false,
        },
        Item {
            topic: "cc/kitchen/pressure",
            retain: false,
        },
    ];
    const BINARIES: [Item; 1] = [Item {
        topic: "cc/kitchen/waterTankFull",
        retain: true,
    }];

    fn plan() -> Plan<'static> {
        Plan {
            parameters: &PARAMS,
            sensors: &SENSORS,
            binary_sensors: &BINARIES,
        }
    }

    /// Every topic a single complete pass offers, in order.
    fn one_pass<'a>(plan: &Plan<'a>) -> Vec<&'a str> {
        let mut cursor = Cursor::new();
        let mut out: Vec<&'a str> = Vec::new();
        while let Some(item) = plan.next(&mut cursor) {
            out.push(item.topic);
        }
        out
    }

    #[test]
    fn a_pass_visits_every_item_exactly_once_in_phase_order() {
        assert_eq!(
            one_pass(&plan()),
            vec![
                "cc/kitchen/brewSetpoint",
                "cc/kitchen/steamSetpoint",
                "cc/kitchen/pidON",
                "cc/kitchen/temperature",
                "cc/kitchen/pressure",
                "cc/kitchen/waterTankFull",
            ]
        );
    }

    #[test]
    fn a_second_pass_repeats_the_same_sequence() {
        // The C++ resets all three cursors and `publishPhase_` to 0 at
        // `MQTTManager.cpp:571-573` when the binary-sensor phase runs out.
        let plan = plan();
        assert_eq!(one_pass(&plan), one_pass(&plan));
    }

    #[test]
    fn a_budget_expiry_resumes_at_exactly_the_next_item() {
        // This is the property the 10 ms budget depends on: stop anywhere, and
        // the next call continues with the item that was not published. If it
        // did not, a pass would either skip a topic or publish one twice, and
        // a skipped topic is a value Home Assistant never learns.
        let plan = plan();
        let mut cursor = Cursor::new();
        let first = plan.next(&mut cursor).expect("first item");
        assert_eq!(first.topic, "cc/kitchen/brewSetpoint");
        // "Budget expired after one item."
        let second = plan.next(&mut cursor).expect("second item");
        assert_eq!(second.topic, "cc/kitchen/steamSetpoint");
        let third = plan.next(&mut cursor).expect("third item");
        assert_eq!(third.topic, "cc/kitchen/pidON");
    }

    #[test]
    fn stopping_at_every_possible_point_and_resuming_never_skips_or_repeats() {
        // Exhaustive over all 7 stopping points of a 6-item plan: the
        // concatenation of the two halves must equal one pass.
        let plan = plan();
        let full = one_pass(&plan);
        for split in 0..=full.len() {
            let mut cursor = Cursor::new();
            let mut seen: Vec<&str> = Vec::new();
            for _ in 0..split {
                seen.push(plan.next(&mut cursor).expect("item").topic);
            }
            // Budget expired. The next iteration resumes.
            while let Some(item) = plan.next(&mut cursor) {
                seen.push(item.topic);
            }
            assert_eq!(seen, full, "split at {split}");
        }
    }

    #[test]
    fn a_phase_with_no_items_is_skipped_without_stalling() {
        // A machine with no pressure sensor and no binary sensors: one pass
        // must still terminate and must not burn three iterations doing it.
        const ONE: [Item; 1] = [Item {
            topic: "cc/kitchen/brewSetpoint",
            retain: true,
        }];
        let plan = Plan {
            parameters: &ONE,
            sensors: &[],
            binary_sensors: &[],
        };
        assert_eq!(one_pass(&plan), vec!["cc/kitchen/brewSetpoint"]);
    }

    #[test]
    fn an_empty_plan_terminates_immediately() {
        let plan = Plan {
            parameters: &[],
            sensors: &[],
            binary_sensors: &[],
        };
        assert!(plan.is_empty());
        let mut cursor = Cursor::new();
        assert!(plan.next(&mut cursor).is_none());
        // And again, so a caller that polls every tick does not spin inside
        // this function.
        assert!(plan.next(&mut cursor).is_none());
    }

    #[test]
    fn parameters_and_binary_sensors_are_retained_and_sensors_are_not() {
        // MQTTManager.cpp:492 retains parameters, :511 does not retain the
        // polled sensors, :545 retains binary sensors. Getting this backwards
        // for the sensors fills the broker's retained store with a value that
        // is stale by the time anyone reads it.
        let plan = plan();
        for item in plan.parameters {
            assert!(item.retain, "{}", item.topic);
        }
        for item in plan.sensors {
            assert!(!item.retain, "{}", item.topic);
        }
        for item in plan.binary_sensors {
            assert!(item.retain, "{}", item.topic);
        }
    }

    #[test]
    fn the_offered_count_tells_a_log_line_whether_the_machine_is_keeping_up() {
        let plan = plan();
        let mut cursor = Cursor::new();
        for _ in 0..3 {
            let _ = plan.next(&mut cursor);
        }
        assert_eq!(cursor.offered(), 3);
        let _ = plan.next(&mut cursor);
        assert_eq!(cursor.offered(), 4);
        cursor.reset();
        assert_eq!(cursor.offered(), 0);
        assert_eq!(cursor.phase(), Phase::Parameters);
        assert_eq!(cursor.index(), 0);
    }

    #[test]
    fn the_cursor_reports_the_phase_it_is_in() {
        // MQTTManager.cpp:410/500/520 are the three phase heads, and a log line
        // naming the phase is how the C++'s "published 12/48" style message is
        // made diagnosable.
        let plan = plan();
        let mut cursor = Cursor::new();
        assert_eq!(cursor.phase(), Phase::Parameters);
        // The phase advances when the *next* call crosses into it, not when the
        // last item of a phase is handed out -- the C++ behaves the same way
        // (`publishPhase_` is set at :571, after the while loop exits).
        for _ in 0..3 {
            let _ = plan.next(&mut cursor);
        }
        assert_eq!(cursor.phase(), Phase::Parameters);
        assert_eq!(cursor.index(), 3);
        let _ = plan.next(&mut cursor);
        assert_eq!(cursor.phase(), Phase::Sensors);
        // One more call leaves the sensors phase; the phase is still Sensors
        // because the binary item has not been handed out yet.
        let _ = plan.next(&mut cursor);
        assert_eq!(cursor.phase(), Phase::Sensors);
        let _ = plan.next(&mut cursor);
        assert_eq!(cursor.phase(), Phase::BinarySensors);
        // One more call finds the end and wraps.
        assert!(plan.next(&mut cursor).is_none());
        assert_eq!(cursor.phase(), Phase::Parameters);
    }
}
