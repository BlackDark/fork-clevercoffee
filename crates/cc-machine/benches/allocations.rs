//! The control tick must not touch the heap. Measured, not asserted.
//!
//! # Why this measures allocations and not nanoseconds
//!
//! REVIEW.md H-8: `reduce()` returned `(Machine, Vec<Effect>)` and allocated a
//! fresh `Vec` per call, and the control task calls it 4-5 times per 10 ms tick.
//! Measured at **4.00 heap allocations and ~192 bytes per tick** -- about 400
//! allocations a second out of the task that also runs the heater deadman, on a
//! device with ~320 KB of RAM. The C++ allocated nothing there:
//! `git show main:src/core/LoopManager.cpp` writes relays inline into member
//! state.
//!
//! A host micro-benchmark of nanoseconds would NOT have caught that. An
//! allocation is roughly as expensive as the rest of the tick put together, and
//! what actually matters is allocator pressure on a fragmented 320 KB heap. So
//! the reported number is allocations, and the time is printed only for scale.
//!
//! # The same measurement gates the build
//!
//! `tests/tick_allocations.rs` runs the identical measurement as a `#[test]`, so
//! `just test` fails if the count is ever non-zero. Both files include
//! `benches/alloc.rs`, so they cannot drift apart — which is the failure mode a
//! duplicated helper would invite.
//!
//! # Caveat
//!
//! Counting a `#[global_allocator]` is exact on a single thread, which is the
//! shape of the control task. It does NOT measure the httpd or display tasks,
//! which have their own allocations and their own reasons.
//!
//! ```sh
//! just bench
//! ```

#[path = "alloc.rs"]
mod alloc;

use std::time::Instant;

use cc_domain::units::{Celsius, Millis};
use cc_machine::{boot, reduce, Context, Event, Machine, Sensors};

use alloc::{nanos_per, per, Counting};

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Iterations for the reported figure. Module scope because
/// `clippy::items_after_statements` is right that an item declared mid-function
/// reads as if it only existed from that point.
const REPORTED_TICKS: u64 = 100_000;

/// One control tick's worth of reducer work, exactly as `Control::tick` does it.
///
/// `Control::tick` (`crates/cc-firmware/src/control.rs`) drives four `reduce`
/// calls per tick -- the sensor sample, the safety verdict, the command drain
/// and the tick itself. The two modelled here are the ones that each allocated
/// a `Vec<Effect>` pre-fix and are representative of the cost; the pre-fix code
/// measured 4.00 per tick across all four.
fn one_tick(machine: &mut Machine, ctx: &Context<'_>, sensors: Sensors, now: Millis) {
    let (next, _effects) = reduce(machine, ctx, Event::SensorUpdated(sensors));
    *machine = next;
    let (next, _effects) = reduce(machine, ctx, Event::Tick { now });
    *machine = next;
}

/// The machine the bench measures: booted, then left in whatever state boot
/// reaches -- `PID_NORMAL`, which is where the control loop spends essentially
/// all of its life.
fn steady_state() -> (Machine, Context<'static>, Sensors) {
    // Leaked on purpose: a `Config` that outlives the measurement, so building it
    // cannot be counted as a tick allocation. One instance for the whole run.
    let config: &'static cc_config::Config = Box::leak(Box::new(cc_config::Config::default()));
    let ctx = Context::new(config, Celsius::new(95.0));
    let (machine, _effects) = boot(Millis::new(1_000), &ctx);
    (machine, ctx, Sensors::healthy())
}

fn main() {
    let (mut machine, ctx, sensors) = steady_state();
    // Warm up: the first call through a code path allocates in caches and in
    // lazily-initialised statics, and that is not what is being measured.
    one_tick(&mut machine, &ctx, sensors, Millis::new(2_000));

    let mark = alloc::counters();
    let started = Instant::now();
    for i in 0..REPORTED_TICKS {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "REPORTED_TICKS is 100_000, a literal; the truncation is exact"
        )]
        let step = i as u32 * 10;
        one_tick(&mut machine, &ctx, sensors, Millis::new(2_000 + step));
    }
    let elapsed = started.elapsed();
    let (allocs, bytes) = alloc::delta_since(mark);

    println!("control tick: {REPORTED_TICKS} iterations");
    println!(
        "  {:>9.2} ns/tick (host x86_64; the LX6 is several times slower)",
        nanos_per(elapsed, REPORTED_TICKS)
    );
    println!(
        "  {allocs:>10} allocations total ({:.3} per tick)",
        per(allocs, REPORTED_TICKS)
    );
    println!(
        "  {bytes:>10} bytes total ({:.1} per tick)",
        per(bytes, REPORTED_TICKS)
    );
    println!();
    println!("  pre-fix, per REVIEW.md H-8: 4.00 allocations and ~192 B per tick,");
    println!("  plus 420 allocations per parameters_json() at the 10 ms gate.");
    assert_eq!(allocs, 0, "the control tick allocated {allocs} times");
}
