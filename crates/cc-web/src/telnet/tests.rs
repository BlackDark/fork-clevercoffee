//! Tests for [`crate::telnet`], the portable half of the Wi-Fi log stream.
//!
//! What is being pinned here is mostly the *numbers* — the shed floor, the ring
//! capacity, the one-client cap, the flush bound. Those are judgements someone
//! will otherwise "tidy" (to 32768, to `RING_ENTRIES / 2`, to "why not allow two
//! terminals") without reading the ADR or the C++ header that explains them.

use alloc::format;
use alloc::string::{String as StdString, ToString as _};
use alloc::vec::Vec;

use cc_config::predecessor::{startup_notice, PredecessorProbe};

use super::*;

/// `Logger.h:159-161`. The ring is 16 entries of 256 B, not a knob.
#[test]
fn the_ring_is_the_csqs_sixteen_by_two_fifty_six() {
    assert_eq!(RING_ENTRIES, 16);
    assert_eq!(ENTRY_BYTES, 256);
}

/// `Logger.cpp:133-152`, `Logger.h:145`.
#[test]
fn the_stream_constants_are_the_csqs() {
    // `Logger.h:146` HEARTBEAT_INTERVAL_MS.
    // `Logger.h:145` MAX_WIFI_FLUSH_PER_UPDATE.
    assert_eq!(MAX_FLUSH_PER_PASS, 8);
}

/// `Logger.h:154`: one `WiFiClient client_`, and a second connection replaces
/// the first (`Logger.cpp:134-137`). This is the constant that stops a debug
/// channel from becoming the OOM, so it gets its own test rather than riding
/// along with the numbers above.
#[test]
fn one_client_is_the_cap() {
    assert_eq!(MAX_CLIENTS, 1);
}

/// `Logger.cpp:161-177` `getLevelString`.
#[test]
fn the_level_words_are_the_csqs() {
    assert_eq!(Level::Trace.as_str(), "TRACE");
    assert_eq!(Level::Debug.as_str(), "DEBUG");
    assert_eq!(Level::Info.as_str(), "INFO");
    assert_eq!(Level::Warning.as_str(), "WARNING");
    assert_eq!(Level::Error.as_str(), "ERROR");
}

/// The whole of ADR-0002 decision 5's observable behaviour, on both edges — which
/// the device version could not assert, because a device test can only observe
/// the branch the real heap happened to be on.
#[test]
fn the_shed_engages_below_the_floor_and_reports_each_edge_once() {
    let mut shed = Shed::new();
    assert!(!shed.is_shedding());

    assert_eq!(shed.decide(200_000), Decision::Allow);
    assert!(!shed.is_shedding());

    // One below the floor: shed, and say so.
    assert_eq!(shed.decide(SHED_FLOOR_BYTES - 1), Decision::ShedEngaged);
    assert!(shed.is_shedding());

    // Still below: shed, and say nothing, or an oscillating machine prints a
    // line per transition.
    assert_eq!(shed.decide(0), Decision::Shed);
    assert_eq!(shed.decide(SHED_FLOOR_BYTES - 1), Decision::Shed);

    // Exactly at the floor is above it: `Logger.cpp:29` is `>=`.
    assert_eq!(shed.decide(SHED_FLOOR_BYTES), Decision::AllowRecovered);
    assert!(!shed.is_shedding());

    // And once recovered it is quiet again.
    assert_eq!(shed.decide(200_000), Decision::Allow);
}

/// `Logger.cpp:29` is `currentFreeHeap() >= MIN_HEAP_FOR_WIFI_LOG`.
#[test]
fn the_shed_boundary_is_inclusive() {
    let mut shed = Shed::new();
    assert_eq!(shed.decide(SHED_FLOOR_BYTES), Decision::Allow);
    assert_eq!(shed.decide(SHED_FLOOR_BYTES - 1), Decision::ShedEngaged);
}

#[test]
fn a_ring_is_empty_to_begin_with() {
    let mut ring = Ring::new();
    assert!(!ring.has_line());
    assert_eq!(ring.pop(), None);
    assert_eq!(ring.pushed(), 0);
    assert_eq!(ring.dropped(), 0);
}

#[test]
fn a_pushed_line_comes_back_unchanged_and_in_order() {
    let mut ring = Ring::new();
    assert!(ring.push("first\r\n"));
    assert!(ring.push("second\r\n"));
    assert_eq!(ring.pop().as_deref(), Some("first\r\n"));
    assert_eq!(ring.pop().as_deref(), Some("second\r\n"));
    assert_eq!(ring.pop(), None);
    assert_eq!(ring.pushed(), 2);
}

/// The property the control tick depends on: a client that has stopped reading
/// costs the producer a bounded amount of work and a counter, never a wait.
#[test]
fn a_full_ring_drops_the_newest_line_and_counts_it() {
    let mut ring = Ring::new();
    for index in 0..RING_ENTRIES {
        assert!(ring.push(&format!("line {index}")), "line {index}");
    }
    assert!(!ring.push("one too many"));
    assert_eq!(ring.dropped(), 1);

    // The oldest survive, which is what makes the ring a backlog rather than a
    // slot: a client that was away for a moment comes back to recent history.
    let mut seen: Vec<StdString> = Vec::new();
    while let Some(line) = ring.pop() {
        seen.push(line.to_string());
    }
    assert_eq!(seen.len(), RING_ENTRIES);
    assert_eq!(seen[0], "line 0");
    assert_eq!(seen[RING_ENTRIES - 1], "line 15");
    assert_eq!(ring.pushed() as usize, RING_ENTRIES);
}

/// Draining and refilling the same ring, which is the steady state of a
/// long-running stream and the case where a slot index wraps.
///
/// Pushed and popped one at a time, so the ring never accumulates and nothing
/// is dropped — which is the assertion that matters: a wrapped ring must not
/// reorder or lose a line that was accepted.
#[test]
fn a_ring_wraps_without_losing_or_reordering_a_line() {
    let mut ring = Ring::new();
    let mut received: Vec<u16> = Vec::new();
    for index in 0..(RING_ENTRIES * 5) {
        assert!(ring.push(&format!("{index}")));
        if let Some(line) = ring.pop() {
            received.push(line.parse().unwrap());
        }
    }
    let expected: Vec<u16> = (0..u16::try_from(RING_ENTRIES * 5).unwrap()).collect();
    assert_eq!(received, expected);
    assert_eq!(ring.dropped(), 0);
}

/// The case a wrapped ring actually reaches in the field: the backlog is
/// deeper than the ring, so the newest line is dropped and the oldest survive.
#[test]
fn a_backlog_deeper_than_the_ring_keeps_the_oldest_and_drops_the_newest() {
    let mut ring = Ring::new();
    for index in 0..(RING_ENTRIES * 3) {
        let _ = ring.push(&format!("line {index}"));
    }
    assert_eq!(ring.dropped() as usize, RING_ENTRIES * 2);
    let first = ring.pop().expect("the oldest line survives");
    assert_eq!(first.as_str(), "line 0");
    assert_eq!(ring.pop().as_deref(), Some("line 1"));
}

/// An over-long line is truncated at the entry's end rather than split across
/// two, and the truncation is visible.
#[test]
fn an_over_long_line_is_truncated_to_one_entry() {
    let mut ring = Ring::new();
    let body = "x".repeat(ENTRY_BYTES * 2);
    assert!(ring.push(&body));
    let line = ring.pop().expect("one line");
    assert_eq!(line.len(), ENTRY_BYTES);
    assert_eq!(ring.pop(), None, "and not two");
}

#[test]
fn a_line_carries_a_stamp_a_level_the_target_and_a_crlf() {
    // The C++'s `[HH:MM:SS] [LEVEL] message\r\n` shape, `Logger.cpp:212`.
    let text = line(Level::Warning, 1_234, "cc_firmware", "pump stuck");
    assert_eq!(text, "[1234] [WARNING] cc_firmware: pump stuck\r\n");
}

#[test]
fn a_line_truncates_with_the_csqs_marker() {
    // `Logger.cpp:221-228`.
    let body = "y".repeat(ENTRY_BYTES);
    let text = line(Level::Info, 0, "t", &body);
    assert_eq!(text.len(), ENTRY_BYTES);
    assert!(text.ends_with("...\r\n"), "{text}");
    assert_eq!(&text[..10], "[0] [INFO]");
}

/// `heapless::String::push_str` fails rather than growing, so this is the case
/// that would otherwise be a silent line loss.
#[test]
fn a_line_that_exactly_fits_is_not_truncated() {
    let prefix = "[0] [INFO] t: \r\n".len();
    let body = "z".repeat(ENTRY_BYTES - prefix);
    let text = line(Level::Info, 0, "t", &body);
    assert_eq!(text.len(), ENTRY_BYTES);
    assert!(!text[..ENTRY_BYTES - 2].ends_with("..."));
}

#[test]
fn an_empty_body_still_produces_a_well_formed_line() {
    assert_eq!(line(Level::Error, 7, "t", ""), "[7] [ERROR] t: \r\n");
}

/// The boot lines about a previous firmware's NVS reach a telnet reader whole.
///
/// `cc_config::predecessor::startup_notice` is the sentence and
/// `cc_config::blob_store` is the namespace; this is the only place both halves
/// and the real formatter meet, which is why the check lives here rather than
/// being a hand-copied byte budget inside `cc-config` that a rename of the
/// target or a change to `ENTRY_BYTES` would silently invalidate.
///
/// The inputs are the worst case: the widest `u32` uptime, the longest level
/// word, and the module that actually logs it. An operator whose machine is on
/// the wrong network reads these over telnet, and a truncated line loses the
/// instruction to re-enter the SSID.
#[test]
fn the_predecessor_boot_lines_are_not_truncated_on_the_wire() {
    for probe in [PredecessorProbe::Populated, PredecessorProbe::Unreadable] {
        let Some(body) = startup_notice(probe) else {
            panic!("{probe:?} must produce a line");
        };
        let text = line(
            Level::Warning,
            u32::MAX,
            "cc_firmware::network",
            &format!("config: {body}"),
        );
        assert!(text.ends_with("\r\n"), "{text}");
        assert!(
            !text[..text.len() - 2].ends_with(ELLIPSIS),
            "the {probe:?} line is truncated to {} B: {text}",
            text.len(),
        );
    }
}
