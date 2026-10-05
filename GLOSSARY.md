# Glossary

**The words this repository uses that mean nothing outside it.** CleverCoffee
descends from a C++ firmware; a lot of the vocabulary came across with it, and
some of it is now misleading precisely because the thing it referred to is gone.

Everything here is defined by what it means, not by how it is implemented. For
how the firmware is built, read [`docs/architecture.md`](docs/architecture.md).
For what works today, read [`docs/status.md`](docs/status.md).

---

## The machine

**Machine state** — one of 18 named situations the machine can be in
(`cc_domain::MachineState`), from `Init` through `PidNormal` and `BrewRunning` to
the error states. The number of states is not a design goal; it is what the
original firmware had, and the port carries them because their transitions are
the behaviour that had to be preserved.

**Reducer** — the pure function that turns a machine state and one event into a
new state plus a list of effects. No I/O, no clock, no hardware: `reduce(machine,
context, event) -> (machine', effects)`. It is the whole control logic, and it
is host-testable for exactly that reason.

**Effect** — one instruction the reducer hands back for the hardware layer to
carry out. `EnablePump`, `CloseWaterValve`, `SetHeaterDuty`. A reducer may only
*ask*; `cc-hal-esp32::actuators` is the only thing that writes a pin.

**Tick** — one iteration of the 10 ms control loop. A tick is not one `reduce`
call; it is a sequence of them, draining the event inbox and then applying one
final `Tick` event, followed by applying the effects.

**Action request** — a request from a button, the web UI, MQTT or telnet that
the machine may or may not act on. A request is not a command: a state that
cannot act on one must *drain* it, so it cannot be picked up later by a state
that would.

## The port

**The C++ firmware** — the firmware this one replaces, deleted from the
repository when the port became the product. Recoverable at
`git show 9fa8c834:<path>`. Where it still matters, that is deliberate: several
behaviours here exist to be *different* from it, and the difference is recorded.

**Divergence** — a place where this firmware deliberately behaves differently
from the C++. "Deliberately" is the operative word. Each one is recorded in
[`docs/history/divergences.md`](docs/history/divergences.md) with the C++'s
behaviour, this one's, and the reasoning.

**Ledger** — the machine-readable half of the divergence record: a set of
declarations, each naming the prose section it belongs to and the diff lines it
explains. It lives *inside* the prose document rather than beside it, so it
cannot drift from the reasoning it claims to encode. `cc-parity` reads it, and
an unexplained diff fails the build.

**Parity** — equivalence with the C++ on a committed set of scenarios. Parity is
measured, not asserted: see [`docs/history/scenario-format.md`](docs/history/scenario-format.md).
The C++ baseline is empty, so **parity has never actually been demonstrated on
this machine.** What the harness does today is prove the Rust scenarios still
run and the ledger is still readable.

**Scenario** — one named, reproducible situation fed to the reducer: a cold
boot, a sensor read, a heater-gate check. Each records one *observation*, a
short structured description of what the machine did.

**Oracle** — historically, the frozen C++ tree that new behaviour was compared
against. The C++ is gone, so the word survives in only two places: the *display
oracle*, a C++ harness that links the real U8g2 so the Rust renderer can be
checked against it, and [`docs/history/recovered-oracle.md`](docs/history/recovered-oracle.md),
a Rust firmware recovered from a flash dump whose source never existed.

## The safety layer

**Safety** — the rule that an actuator may be energised only in a state that is
allowed to energise it, enforced in one place (`cc_safety`). This runs every
tick and can refuse what the state machine asked for. That is the point: it is
the backstop under the control logic, not part of it.

**Whitelist** — the list of states permitted to hold a valve open. Water and
steam have separate ones, and the separation matters because **both valves share
a single relay**, so an ungated steam valve is an ungated water valve.

**Fail-closed** — the design rule for anything whose failure energises
something. A heater relay that is `LOW_TRIGGER` is not "inverted" and made to
work; it is *refused*, because an undriven GPIO at reset would turn it on.

## Working on it

**The gate** — the checks that must pass before anything is committed. `just
check` is the host gate and needs no hardware; `just gate` adds the device
build and the size budget. Both are defined in [`AGENTS.md`](AGENTS.md).

**Claim** — an assertion that something works, together with the commit or
measurement that backs it. [`docs/status.md`](docs/status.md) is the only page
allowed to make one, and every line in it is a pointer rather than a conclusion.
A claim nobody can follow is a wrong claim.

**Dormant** — a schema key, an endpoint or a declared divergence that exists but
has never been exercised. Dormant is not broken and it is not verified; it is
the third thing, and the honest word for it.
