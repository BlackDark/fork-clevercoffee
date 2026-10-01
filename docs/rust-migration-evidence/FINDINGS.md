# Findings

Read time: 5 minutes. Purpose: what works, what did not, and how each was earned.

**TL;DR**

1. Only `rewrite/rust` has run on a board. Everything else is inference from source.
2. The port found 9 defects no document predicted. Two of them meant the machine could not heat at all.
3. The binding constraint is RAM, not flash. Static use is 41 % of 320 KB.

Grade = how the fact was earned. Read the grade, not the prose.

| Grade | Meaning |
|---|---|
| ✅ | Reproduced on the attached ESP32, with a test pinning it |
| ⚠️ | Found by two or more branches from source; sound, unconfirmed |
| ❓ | Single source, or observed with no known cause. Do not treat as understood |

## Works

| Feature | Status | Branch | Evidence |
|---|---|---|---|
| Build, flash, boot | ✅ | rewrite | `893d13dd`, 382 KB blink image |
| Wi-Fi station + join + provisioning | ✅ | rewrite | `fde5e776` — WPA2-only naming was the fix |
| HTTP API, 23 routes | ✅ | rewrite | 150/150 `200`, p50 67 ms, device |
| SSE live events | ✅ | rewrite | `2163de5`; `scripts/sse-starvation-check.py` |
| Web UI served from flash | ✅ | rewrite | `46d03406`, 199 KB embedded bundle |
| `POST /api/parameters`, 98 writable | ✅ | rewrite | `e9de28b4`, survives reboot |
| Config import/export round trip | ✅ | rewrite | 2077 B vs oracle's 2071 B |
| OLED display, pixel parity | ✅ | rewrite | 11 scenarios, zero differing pixels |
| PID control | ✅ | rewrite | 47.4 % duty at 7 K error on the board |
| Heater chopper, 10 ms ISR | ✅ | rewrite | 42 ticks / 420 ms = exactly 100 Hz |
| State machine, 18 states | ✅ | rewrite | 4140-pair exhaustive table |
| DS18B20 1-Wire probe | ✅ | rewrite | family `0x28`, ROM `2869...af41`, on board |
| HX711 scale | ✅ | rewrite | `b5d1c0b9` — a feature the C++ never had |
| NVS storage | ✅ | rewrite | `cc-hal-esp32/src/nvs.rs` |
| Telnet log server | ✅ | rewrite | 16 × 304 B ring |
| UART0 provisioning | ✅ | rewrite | `7f51583` — needed a 16 KB stack |
| Task watchdog (TWDT) | ✅ | rewrite | control task is the only subscriber |
| Heater deadman interlock | ✅ | rewrite | `cc-domain/src/heater.rs` — 500 ms interlock, 1000 ms deadman. **Not in the C++ at all** |
| GPIO relays, switches, LEDs | ✅ | rewrite | `cc-hal-esp32/src/actuators.rs` |
| I²C (OLED), SPI (HX711, pressure) | ✅ | rewrite | `sensors.rs`, `scale.rs` |
| MQTT client | ⚠️ | rewrite | client built; on-device publish not re-verified after the `plan()` panic fix (`4f213473`) |
| ZACwire / TSIC-306 device path | ❌ | rewrite | written, never brought up; the pin carries 1-Wire |
| TSIC-306 decoding | ❓ | rewrite | simulation only. A green run is not a working sensor |
| BLE / Acaia scale | ❌ | rewrite | `2013bda9` — costs +205 KB flash, +40 KB RAM. Not built |
| HTTP OTA | ❌ | rewrite | routes answer `501` (`web.rs:1810`) |
| Sleep / deep sleep | ❓ | both | neither C++ nor Rust uses it. Not a gap |
| ADC | ❓ | both | neither uses the ADC peripheral |

## Not as expected

| Problem | Symptom | Cause | Workaround | Status |
|---|---|---|---|---|
| FP in an ISR | `EXCCAUSE 0x4` panic on first fire | Xtensa does not save FPU state across an interrupt | Keep duty arithmetic integer; `CONFIG_FREERTOS_FPU_IN_ISR` stays off | ✅ `51fa96ca` |
| LEDC at 1 Hz | Panics every boot | `ledc_ll_set_duty_start` spins inside a critical section | 10 ms GPTimer ISR. `LedcPwm` behind a const assert | ✅ `42be3578` |
| Water tank switch inverted | Stuck in `WaterTankEmpty`, PID cleared | `raw` returns false when **no** float is fitted | Read the fitted flag | ✅ |
| PID never computed | `P=0.0 I=0.0 D=0.0` at 7 K error | Mode cache seeded from intent, controller starts in Manual | API that cannot be seeded wrongly | ✅ |
| Framebuffer on an 8 KB stack | "stack overflow in task pthread" | 1 KB buffer on the control task | Allocate once in bring-up | ✅ `0815710c` |
| `app_main` stack too small | Allocator assert naming nothing | 3584 B against ~11 KB of bring-up | Bring-up on its own 16 KB thread | ✅ |
| No `init_stack()` | `tcpip_send_msg_sem (Invalid mbox)` | Radio bring-up skipped, so lwIP never started | Call it unconditionally | ✅ |
| SSE on the httpd task | 150/150 requests timed out | ESP-IDF httpd is one task for the whole server | Handler registers and returns; a broadcaster task writes | ✅ `648d0ac8` |
| OTA routes present but dead | 501 on every mutating route | Not implemented | Honest answer names the owning task | ✅ |
| Password window lasted 0 ms | Every password line parsed as a command | `wrapping_sub` comparison is true for the window's whole life | Revert the broken form to prove the test fails | ✅ |
| MQTT `plan()` panic | Chip reset on first publish | Per-group lengths used as absolute offsets | Covering test had vacuous assertions | ✅ |
| Endless read | 192 KB allocation, chip down | A fake reader returned its first chunk forever | Bounded buffer | ✅ |
| WPA3 mask | Station would not join a WPA2-only AP | `wifi_auth_mode_t` is a sequence compared for equality | Name WPA2, do not widen the mask | ✅ `fde5e776` |
| Non-Latin-1 glyph | Display task aborted | Unbounded glyph walk from an HTTP-sourced string | `tests/text_safety.rs` | ✅ `495676ec` |
| Scale watchdog | Healthy scale, measuring nothing | Armed on first conversion; `note_ready` never fires | Arm at driver start | ✅ |
| Frame hand-off race | Producer at 100 Hz wrapped the reader's slot | Two buffers, no synchronisation | Seqlock | ✅ `2b60de8` |
| Divergence ledger is incomplete | A deliberate divergence is absent from it | The code points at `intentional-diffs.md` #12, which is the hostname | Declare it or drop the behaviour | ❌ open |
| The ledger is append-only | §13, §14 and §15 each appear **twice** | Corrections appended rather than rewritten in place | Read by heading, not by number | ⚠️ |
| Tick runs at ~65 Hz, not 100 Hz | Deadline missed on every tick | ❓ 12 ms mean inside the **act** span alone. Not the 1-Wire, not the reducer, not the display | — | ❓ open, not a safety regression |
| Early tick figures, superseded | 62 % of ticks over budget, worst 32 ms | The 1 KB display frame was drawn **inside** the tick | Panel moved to its own task, 100 ms (`158f61b5`) | ✅ resolved |

## ESP32 pitfalls checklist

- [ ] No floating point in an ISR. No compile-time warning exists.
- [ ] Heater relay on GPIO2, a boot strapping pin, driven late. `space2` moves it to GPIO4.
- [ ] Four panel switches on input-only pins 34/35/36/39 need external pull-ups.
- [ ] Steam LED on GPIO1, UART0 TX. The comment says it moved. It did not.
- [ ] I²C runs at the Arduino default 100 kHz. Share the bus behind a mutex.
- [ ] `delay_ms(n)` is quantised to a 10 ms grid at `CONFIG_FREERTOS_HZ=100`. Size assertions to the grid.
- [ ] The millisecond clock wraps every 49.7 days. Subtract elapsed time with wrapping.
- [ ] `esp_restart()` drops unflushed UART0 output. Gate `uart_wait_tx_done` on the driver.
- [ ] `esp_restart()` reports `SW`, `abort()` does not. Accept anything except POWERON/EXT.
- [ ] Fitted probe is a **DS18B20**, not the TSIC-306 the config defaults to.
- [ ] The DS18B20 powers on reporting 85 °C, and at reduced resolution the low bits are stale.
- [ ] Relay coils must not draw from the dev board's 3.3 V rail. Needs ≥500 mA.

## Two timing traps in the tick instrumentation

Both are recorded in `rewrite:docs/rust-migration/09-cpp-findings.md` §24 and §31, and both cost an afternoon each.

- **Accumulate section deltas at the end of the tick** and the spans come out nested, not disjoint. Every section then reports the same number.
- **Read the totals after zeroing them** and every section prints zero while the tick is 12 ms. That points at the clock, not the code.

## Size and timing

RAM is the binding constraint. Flash had the margin.

| Metric | Value | Evidence |
|---|---|---|
| Static RAM | 131 688 B = 41 % of 320 KB | `rewrite` link map |
| Prebuilt Wi-Fi MAC in `.iram0` | 94 155 B, not Rust's to shrink | `CONFIG_ESP_WIFI_IRAM_OPT=y` |
| App image | 1 338 816 B of a 1 835 008 B slot = 72.9 % | `just size` |
| C++ image for comparison | 1 546 240 B of 1 703 936 B = 90.4 % | `pio run` |
| Removing the `std` backtrace | −162 656 B flash, −1 440 B RAM | `nm` symbol count 148 → 0 |
| U8g2 fonts as RLE vs `ImageRaw` | 42 722 B vs 177 723 B | `rewrite` link map |
| `esp-hal` bare-metal image | 99 728 B | `space2` SPIKE |
| `esp-idf-svc` spike, all subsystems | 997 648 B | `design` SPIKE-1 |
| Reachable LEDC resolutions at 1 Hz from 80 MHz | 17, 18, 19, 20 bits only | 16 bits overflows the max divider |
| C++ pressure read | 10 ms blocking every 50 ms = 20 % of the loop | `pressureSensor.h:35` |
| Display full-page flush | 1024 B per render, ~90 ms blocked | `CONFIG_ESP32` I²C at 100 kHz |
| Device tests on hardware | 111, all registered and passing the audit | `just test-esp32` |
| Host tests | 900+ | `just test` |

## Unverified

| Claim | Why it is unverified |
|---|---|
| The control tick's 12 ms applier span | Nothing in that span obviously blocks. Next step: split `apply` / `drain_scale` / reboot checks |
| Contactor minimum on/off time | Needs a scope and a dummy load. Unmeasured on any branch |
| Heater relay polarity on the installed machine | A meter, boiler disconnected, settles it |
| Display slots that overflow | Five strings exceed their slot; a product decision on typography |
| The 1 px message-screen overlap | Six lines at 11 px is 66 px into a 64 px panel |
| A LOST device test | No gate fails on it. Pre-existing, nobody has looked |
| C6 and S3 behaviour | `space2`'s board crates have never seen a compiler |
| Multi-drop 1-Wire enumeration | Acceptable only because a real machine has one sensor |
| Whether the C++ also runs at ~65 Hz | The per-iteration histogram is recorded at R0-04 but never compared |

## Verification lessons

- A missing test command is a shipped bug. 68 `#[test]`s in `cc-hal-esp32` were type-checked and run by nothing.
- `cargo test` cannot work on this target. `panic = "abort"` is forced; unwinding does not link on Xtensa LX6.
- A timing instrument that has never disagreed with a result is not known to be working.
- A gate that fails on formatting teaches people to ignore gates.
- An empty baseline is an error, not a skip. The runner reports `BASELINE-MISSING` and exits 2.
- A ported defect is not retired by being understood. It is retired by a test that fails without the fix.
- One firmware on the chip at a time. Log C++ for ten minutes, log Rust for the same ten, diff offline.
- Every task that energises anything carries a written procedure with the element isolated.
