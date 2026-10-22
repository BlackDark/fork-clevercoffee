//! Gate reducer allocations with the `cc-alloc-count` allocator.
//!
//! This does not cover the device-only shell or other tasks. Set up fixtures
//! outside the counting window and keep counter access within one test.

use cc_domain::units::{Celsius, Millis};
use cc_machine::{boot, reduce, Context, Event, Machine, Sensors};

use cc_alloc_count::Counting;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Iterations. A thousand is enough that a per-tick allocation is impossible to
/// miss and cheap enough that this runs in milliseconds.
const TICKS: u64 = 1_000;

// The inline capacity is a compile-time question: "is it still a sane size for
// a 10 ms tick" cannot change at runtime, and the `284ad17a` fix put the whole
// effect list on the control task's stack.
const _: () = assert!(
    cc_machine::effect::MAX_EFFECTS_PER_EVENT <= 64,
    "the inline capacity has grown past what a 10 ms tick should ever need"
);

fn one_tick(machine: &mut Machine, ctx: &Context<'_>, sensors: Sensors, now: Millis) {
    let (next, _effects) = reduce(machine, ctx, Event::SensorUpdated(sensors));
    *machine = next;
    let (next, _effects) = reduce(machine, ctx, Event::Tick { now });
    *machine = next;
}

#[test]
fn the_control_tick_does_not_allocate() {
    // Leaked so the `Config` cannot be counted as a tick allocation.
    let config: &'static cc_config::Config = Box::leak(Box::new(cc_config::Config::default()));
    let ctx = Context::new(config, Celsius::new(95.0));
    let (mut machine, _effects) = boot(Millis::new(1_000), &ctx);
    let sensors = Sensors::healthy();

    // Warm up: the first call initialises lazily-initialised statics that are not
    // a tick's cost.
    one_tick(&mut machine, &ctx, sensors, Millis::new(2_000));

    let mark = cc_alloc_count::counters();
    for i in 0..TICKS {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "TICKS is 1_000, a literal; the truncation is exact"
        )]
        let step = i as u32 * 10;
        one_tick(&mut machine, &ctx, sensors, Millis::new(2_000 + step));
    }
    let (allocs, bytes) = cc_alloc_count::delta_since(mark);

    assert_eq!(
        allocs, 0,
        "{TICKS} control ticks made {allocs} heap allocations ({bytes} bytes). \
         The reducer must not touch the allocator: `Effects` is a \
         `heapless::Vec` and `Machine` is returned by value on the stack."
    );

    // ---------------------------------------------------------------------
    // The second half of the same guarantee, stated as a type fact: the effect
    // list is INLINE storage, so there is no allocator for it to call even in
    // principle. The `Vec<Effect>` this replaced made an allocation on the
    // control tick possible at all; this is what stops that coming back. The
    // ceiling sanity check is a `const` assertion at module scope, because it
    // is a compile-time question and clippy is right that asserting it at
    // runtime would be dead code.
    // What is NOT a compile-time fact is the size of the type the firmware
    // actually puts on its control-task stack, so that is asserted here.
    let size = core::mem::size_of::<cc_machine::Effects>();
    assert!(
        size <= cc_machine::effect::MAX_EFFECTS_PER_EVENT
            * core::mem::size_of::<cc_machine::Effect>()
            + 64,
        "`Effects` is {size} bytes; it should be inline storage for at most \
         MAX_EFFECTS_PER_EVENT effects plus a counter, not a heap vector"
    );
}
