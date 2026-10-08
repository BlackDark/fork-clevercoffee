# C++ behaviour comparisons

**One place for every "the C++ answered X, this firmware answers Y".**

The runbook has been kept to procedures. Every check in it that exists *because*
the C++ behaved a certain way now points here instead of carrying the comparison
inline, so that the runbook reads as something you do at a machine and this reads
as something you consult before you change an answer.

There are two other places this material lives, and they are not duplicates:

- [`divergences.md`](divergences.md) is where this firmware **deliberately
  differs**, with the reasoning and the test that pins each one. Read it when you
  are about to *change* a behaviour.
- [`../differences.md`](../differences.md) is the 120-line version, for someone
  who wants to know "will this surprise me?" without the detail.

This page is the reverse direction: **parity that was chosen, not parity that
was inherited.** Where the two firmwares agree on something ugly, and that is
still ugly, it is recorded here so nobody "fixes" it by accident.

---

## The agreement is not always agreement

The C++ had bugs. Reproducing one is a decision with a cost, and it is recorded
here so the decision is visible rather than inherited by accident.

| Behaviour | Why it is kept | Ledger |
| --- | --- | --- |
| A reboot boots to `PID_DISABLED` | `hardware.switches.power.type` is `Toggle`, and a toggle reading off at boot starts disabled. This is the C++'s behaviour matched line for line; `pid.enabled` only wins when no power switch is configured. | [`divergences.md`](divergences.md) |
| Error strings used to clip off the panel | No longer kept. | [`divergences.md` §40](divergences.md#d40) |
| The Scale template's rows collide | Both firmwares default `fullscreen_brew_timer` to false, so the collision is live during a brew rather than hidden. Fixing it moves a screen somebody looks at. | [`outstanding-findings.md`](outstanding-findings.md) #1–#2 |
| The steam LED is not driven | GPIO1 is UART TX, so the C++ drove both and the last attach won. The rule is implemented and host-tested; only the pin is absent, and moving it is a hardware change because GPIO32 is the scale's data line. | [`divergences.md`](divergences.md) #30 |

## Where this firmware is safer than the C++

These are not bugs preserved out of affection. They are the reason the port was
worth doing, and each one closes a real gap in the original.

| Behaviour | What the C++ did | What happens now |
| --- | --- | --- |
| Steam valve | No whitelist check existed, and nothing called the function that would have been one. Closed only by the accident that nothing called `openSteamValve()`. | `cc_safety::steam_flow_allowed` closes the valve in every state but `STEAM_RUNNING`. Water and steam share one relay, so this is a water safety fix. |
| Water tank empty | The float switch was read for display purposes but not before pumping. | The pump is gated. |
| Pump timeouts | Both safety timeouts were dead code. | Both are armed, and re-arm after a release. |
| The PID derivative term | Computed from a counter, not real elapsed time. | Taken from real elapsed time. |
| Over-temperature debounce | Counted clock ticks, not probe samples, so a slow probe read could under-count. | Counts samples. |
| HTTP authentication | Wildcard-origin header on every response. | Implemented, and boot-time. |
| Configuration validation | None. An unsafe combination was stored and then ran. | Refused at write time, and re-checked before it runs. |
| `LOW_TRIGGER` heater relay | Selectable in configuration, which means an undriven GPIO at reset energises a 2 kW heater. | Refused outright. It is a wiring property no firmware can make safe. |

## How to add to this page

When you change a behaviour and the C++'s answer is part of the reasoning, add a
row. When the change is a divergence, it belongs in
[`divergences.md`](divergences.md) instead, with a ledger block — and this page
should link to it rather than restate it. The two files must not drift into
telling the same story twice.
