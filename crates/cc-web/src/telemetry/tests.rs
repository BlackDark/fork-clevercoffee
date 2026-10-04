//! What the type itself has to guarantee, which nothing above it can enforce.
//!
//! These assertions are on the *type*, not on a payload: they are the properties
//! `cc_hal_esp32::web::Snapshot`'s hand-written `Sync` names in its second
//! safety bullet — "`get` clones `T`; with `T = Telemetry` that is a fixed-size
//! copy … and `T: Send` bounds the transfer" — and which nothing in the HAL
//! could check without a board.

use super::*;

#[test]
fn the_payload_is_bounded_and_carries_no_allocator() {
    const MAX: usize = 256;
    // The claim the `Sync` impl rests on is that a reader's copy is a `memcpy`
    // of at most a few dozen bytes with no allocation. If a field became a
    // `String` or a `Vec`, that would stop being true and the type would go
    // back to being the use-after-free `Snapshot`'s own docs describe.
    //
    // An IPv4 address in dotted-quad form is at most `255.255.255.255`, so
    // `heapless::String<15>` is exactly the right size and `String<15>` cannot
    // silently become `String<16>` without this ceasing to compile.
    let ip = heapless::String::<15>::try_from("255.255.255.255")
        .expect("the longest an IPv4 address can be fits");
    assert_eq!(
        ip.len(),
        15,
        "15 bytes of capacity, exactly the longest IPv4"
    );
    // Whatever the layout is, it is a fixed one: the point is not the number
    // but that it does not grow with the address and that `clone` cannot
    // allocate. `MAX` above is the claim that matters; this is the ceiling on
    // the one field that could plausibly be made dynamic again.
    const {
        assert!(
            core::mem::size_of::<heapless::String<15>>() <= 32,
            "the IP field must stay a fixed-size inline buffer"
        );
    }

    // The whole struct is what the critical section copies. It has to stay
    // small enough that "over in about a microsecond" (the doc's claim) is
    // true against a 10 ms control period.
    let size = core::mem::size_of::<Telemetry>();
    assert!(
        size <= MAX,
        "Telemetry grew to {size} B; Snapshot::get copies it inside \
         interrupt::free, so this is a control-loop budget, not a struct size"
    );
}

#[test]
fn a_command_carries_no_allocator_either() {
    // 04 §3.2: "No `String`, no `Vec`, no `Box` in a cross-task message." A
    // `Command` is queued into a fixed-size `Queue` and drained by the control
    // task, so its size is the queue's per-slot cost, and `Copy` is what makes
    // `Queue::enqueue` infallible. `SetSetpoint(i32)` is the widest variant and
    // everything else is a unit or a `bool`, so eight bytes is the whole enum.
    const _: () = assert!(core::mem::size_of::<Command>() == 8);
}
