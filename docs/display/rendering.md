# How a frame reaches the panel

**What `cc-display` does with a frame, and why it allocates nothing.**

The C++ renderer's architecture is preserved in
[`../archive/cpp/display-subsystem-architecture.md`](../archive/cpp/display-subsystem-architecture.md).
This page is the Rust one, because that is what runs.

For the layout rules — what may go where, and what must not clip — read
[`layout-rules.md`](layout-rules.md). For the checks, read
[`parity.md`](parity.md).

---

## One render pipeline

There is one framebuffer, one `DrawTarget`, and one set of shared stages. Each
template is an override of the shared pipeline, not a separate renderer, and the
thresholds a stage uses come from one place so a template cannot disagree with
itself about what "near setpoint" means.

[`ADR-0001`](../../docs/adr/0001-display-subsystem-architecture.md) records the
decision and the alternatives rejected.

## A frame, in order

1. The caller asks for a state — `PidNormal`, `BrewRunning`, and so on.
2. The active template fills the framebuffer: shared stages first, then its
   overrides, then its own widgets.
3. `present()` pushes the framebuffer to the panel.

Step 3 is where the memory budget bites, and the next section is why.

## The I²C bus is shared, and that is the whole constraint

The SSD1306 and the ABP2 differential pressure sensor sit on **one** I²C bus.
A naive frame is 64 bus writes — one per pixel row — and at that rate the
pressure sensor is starved, which shows up as an erratic brew weight rather than
as a display problem.

So a frame is **chunked into 8 bus writes**. That is the number to remember:
changing it is a change to sensor behaviour, not to display performance. The
sensor task also takes the bus behind the same `Mutex`, so a frame never holds it
long enough to delay a reading.

`AG-REPO-12` records this, and the interaction with the event stream is in
[`../web/http-and-ui.md`](../web/http-and-ui.md).

## A frame allocates nothing

`cc-display` is `#![no_std]` and has **no `alloc`** in the device build. An
allocation there is a link error on the chip, not a runtime failure, which is a
much better way to find out.

`crates/cc-display/tests/frame_allocations.rs` asserts zero heap bytes per frame.
The counting allocator it uses deliberately counts the **calling thread**: an
earlier version counted the whole process, so libtest's own allocations landed in
the measurement window and the gate failed 62 of 80 runs under CPU load. Nothing
in `cc-display` was allocating; the instrument was.

## The bitmap table

`cc_display::bitmaps::ALL` names each bitmap and gives its dimensions;
`cc_display::bitmaps_data` holds the bytes. The display parity oracle builds its
own C header from that table at test time rather than carrying a second copy, so
the artwork the oracle draws is the artwork the firmware ships.

There are two traps in the artwork worth knowing, both recorded in
[`../hardware/pins.md`](../hardware/pins.md): the steam LED is on GPIO1, which is
UART TX, and GPIO32 — the alternative — is the scale's data line.

## Fonts

Ten profont/fub atlases, copied verbatim from U8g2 into
`cc-display/src/font/data.rs` rather than linked at runtime. That is a deliberate
flash-versus-RAM trade and it is measured; the reasoning is in the generated
file's own header.

`crates/cc-display/tools/extract_fonts.py check` proves the file still matches
the U8g2 it was extracted from. U8g2 is fetched by `just u8g2` at upstream tag
`2.36.18`.
