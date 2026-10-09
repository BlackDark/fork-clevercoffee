# The state machine

**What decides what the machine does next, and why it is a pure function.**

The control logic is `cc-machine`, and its entire interface is:

```rust
reduce(machine: &Machine, ctx: &Context<'_>, ev: Event) -> (Machine, Effects)
```

No I/O, no clock, no hardware. It takes a state and one event and returns a new
state plus a list of instructions. This is the decision that shapes everything
else in the firmware, and it is why the state machine can be tested with a
plain `cargo test` and no hardware (`just test`).

For the crate boundaries around it, read [`../architecture.md`](../architecture.md).
For the rules that bind changes here, `AG-REPO-21` through `AG-REPO-26` in
[`AGENTS.md`](../../AGENTS.md).

---

## The 18 states

`cc_domain::MachineState`, in discriminant order. The number is not a design
goal; it is what the original firmware had, and the transitions between them are
the behaviour the port had to preserve.

| Discriminant | State | What is energised |
| --- | --- | --- |
| 0 | `Init` | nothing; before the first transition |
| 20 | `PidNormal` | heater, at the setpoint. Idle and ready |
| 31 | `BrewPreinfusion` | pump + water valve, no heat |
| 32 | `BrewPreinfusionPause` | heater only |
| 33 | `BrewRunning` | pump + water valve, PID delayed by `brew.pid_delay` |
| 34 | `BrewFinished` | nothing; post-brew timer |
| 36 | `ManualFlushRunning` | pump + water valve |
| 51 | `SteamRunning` | steam valve |
| 60 | `BackflushIdle` | nothing |
| 61 | `BackflushFilling` | pump + water valve |
| 62 | `BackflushFlushing` | water valve |
| 63 | `BackflushFinished` | nothing |
| 70 | `WaterTankEmpty` | nothing; the pump is blocked |
| 80 | `EmergencyStop` | nothing; latched |
| 90 | `PidDisabled` | nothing |
| 95 | `Standby` | nothing, awaiting a wake |
| 100 | `SensorError` | nothing |
| 110 | `EepromError` | nothing |

The gaps in the numbering are inherited, not lost. They are the discriminants the
C++ used, and the HTTP status API reports them as numbers.

## What a tick actually is

A tick is **not** one `reduce` call. The control task runs at 100 Hz and each
iteration is:

1. Drain the event inbox — button presses, HTTP requests, MQTT commands — and
   `reduce` each one.
2. `reduce` a final `Tick` event carrying the clock reading.
3. Apply the returned effects through `cc-hal-esp32::actuators`.
4. Sample the sensors into the shared snapshot.

Steps 1 and 2 are separate calls because `reduce` handles exactly one event. The
split matters because it is what makes "drain stale request flags" (`AG-REPO-24`)
expressible: a state that cannot act on a request must consume it anyway, or it
will still be in the inbox when a state that *can* act on it comes around.

Nothing in a tick allocates. `crates/cc-machine/tests/tick_allocations.rs`
asserts zero heap bytes, because a control loop that allocates is a loop that
eventually fails to be scheduled.

## The five rules that bind changes here

These are `AG-REPO-21` through `AG-REPO-26` in abbreviated form. The rulebook is
normative; this is the working summary.

**Release on exit, always.** Any state that opens the pump or a valve must close
it in `states::on_exit`. The next state's `on_entry` may not run if an error
interrupts the transition, so the state that opened it has to be the one that
closes it.

**Reinforce on entry.** A state's tick should re-assert what it wants, because
the safety layer may have closed the valve since the last tick. Entry must be
idempotent with respect to hardware.

**Safety can veto, and it runs last.** `cc_safety::water_flow_allowed` and
`steam_flow_allowed` are applied after the reducer's effects, every tick. The
reducer decides what should happen; safety decides what is allowed. Adding a
water-flow state means adding it to the whitelist or it will not work, and you
will not find that out at the machine.

**The PID is off for active operations.** Adding an operational state means
adding it to the exclusion list, or the heater will be fighting the brew.

**Never write a pin.** Return `Effect::EnablePump`, not a GPIO write. The
actuator for opening a valve is a no-op while the relay is off, so a direct write
can leave hardware stuck in a state the bookkeeping does not believe in.

## Adding a state

1. Add the variant to `cc_domain::MachineState`. Give it the discriminant the
   protocol expects, or add a new number deliberately.
2. Decide its `on_entry` and `on_exit`. If it energises anything, `on_exit` must
   release it.
3. If it holds a valve open, add it to `cc_safety::water_flow_allowed` — and to
   `steam_flow_allowed` if it is steam.
4. If it is an operation rather than an idle or error state, add it to the PID
   exclusion list.
5. Write the tests for what it does **and** for what it does on the way out. The
   exit path is the one that matters at the machine.

[`ADR-0003`](../../docs/adr/0003-state-machine-hardware-control-contract.md)
records the contract these rules come from, including the two C++ behaviours that
motivated it.

## A hand on the buttons

Debounce and long-press are pinned by 17 host tests against a synthetic clock.
The 2026-10-09 hand check is on [`status.md`](../status.md). It is not a Problem.
