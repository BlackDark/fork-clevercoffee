# Measurements

The numbers, in one place. Claims live in [findings.md](findings.md); decisions that depend on these
live in [open-decisions.md](open-decisions.md).

Device-verified rows are marked. The rest is measured on the bench device, on the linked ELF, or
derived against the target's installed ESP-IDF source — the "how" column says which.

---

## The binding constraint is RAM, not flash

Every sizing conclusion follows from this. Flash had the dramatic margins; RAM does not.

| Metric | Value | How measured |
|---|---|---|
| **Static RAM** | **131 688 B = 41 % of 320 KB**, up from 62 KB before the network work | link map |
| └ prebuilt Wi-Fi MAC in `.iram0` | **94 155 B** — `libpp.a` / `libnet80211.a`, `CONFIG_ESP_WIFI_IRAM_OPT=y` | link map |
| └ largest single item | `POWER_OF_FIVE_128` = **10 416 B** in `.dram0.data` (7.9 % of static RAM), for one `f64::from_str` | link map |
| DRAM heap available | **~154 KB** of the 320 KB | `esp_get_free_heap_size` |
| `std` backtrace removal: flash | **−162 656 B (−12.05 %)** | `nm` symbol count 148 → 0 |
| `std` backtrace removal: RAM | **−1 440 B (1.1 %)** — the ".bss buffer" theory was **wrong** | link map |
| Image | 1 338 816 B of a 1 835 008 B slot = **72.9 %** | `just size` |

The 94 KB of Wi-Fi MAC IRAM is untouched by any Rust-side change. ADR-0002's 30 KB heap-shed
threshold was tuned against ~75 KB of static use; the margin is now much thinner.

## Flash

| Metric | Value | How measured |
|---|---|---|
| C++ `firmware.bin` | 1 539 657 – 1 546 240 B of a 1 703 936 B slot = **90.4 %**, ~154 KB headroom | `pio run` + `partitions_4M.csv` |
| `esp-idf-svc` spike, every subsystem linked | **997 648 B = 58.5 %** of the 1.625 MB slot | SPIKE-1 |
| `esp-hal` bare metal | **99 728 B** app image from a 2 571 412 B ELF | `espflash save-image` |
| Growth, storage+network phase | +966 816 B (**+252 %**) | attributed with `nm` |
| Growth, display phase | +122 000 B, of which **63.8 KB is U8G2 font tables** | `just size` |
| U8g2 fonts as RLE vs `ImageRaw` | **42 722 B vs 177 723 B = 135 001 B** | link map |
| mbedTLS | **94 545 B / 680 symbols** — `mqtt → tcp_transport → esp-tls`, `esp_wifi → wpa_supplicant` | link map |
| Rebalance formula | slots = `(4 063 232 − S) / 2`; 1 835 008 B slots maximises `min(app0, app1)` | arithmetic |
| "≥ 2 MB per slot" | **arithmetically impossible** — short by 131 072 B | arithmetic |

mbedTLS was investigated and **deliberately kept**: removing it changes which access points the
machine can join, which is a product decision rather than a size optimisation. The CA bundle is not
linked (a 14-byte stub), so that part is already a non-issue.

## Stacks

Size these from `--dwarf=frames`, not from a feeling. Two of these overflowed in practice.

| Stack | Size | Note |
|---|---|---|
| `app_main` | **3 584 B** (`CONFIG_ESP_MAIN_TASK_STACK_SIZE`) | **under a third of what bring-up needs** — overflowed into DRAM as an allocator assert |
| bring-up | **16 KB**, ~11 KB deepest chain | 2400+1584+3088+5×~450+1712 ≈ 11 KB → ~40 % headroom, 10 % of the heap |
| control task | **8 KB** | a 1 KB framebuffer on it reset the chip |
| scale sampler | **4 KB** at priority 6 (one above control) | asserted at compile time |
| SSE broadcaster | **4 KB** | |
| `ControlArgs` ceiling | `Config` 632 B, total asserted **< 2048 B** | const-assert |

## Timing

| Metric | Value | How measured |
|---|---|---|
| Control tick | **400 ms** period, **10 ms** budget | 1 temperature + 20 pressure reads per tick, dividing exactly |
| Tick overruns | **86 of 138** ticks over budget, worst **32 ms** (scale on); **111 of 136** (scale off) | device tick-timing report |
| Overrun cause | **unexplained** — the scale is ruled out; the same worst case appears either way | device |
| C++ pressure read | `delay(10)` on a 50 ms cadence = **20 % of loop asleep** | `pressureSensor.h:35` |
| I²C bus rate | **100 kHz** (Arduino default; `setClock()` never called) | `Wire` defaults |
| Display full-page flush | 1024 B per render, blocking **~90 ms**, against a 100 ms slow-loop threshold | arithmetic |
| `FreeRtos::delay_ms` | quantised to a **10 ms** grid at `CONFIG_FREERTOS_HZ=100`; 20 ms request measured **12 ms** | device |
| Device vs host clock | agrees to **1.4 %** over a 1.27 s boot | device |
| Millisecond wrap | **49.7 days**, with the heater running across it | device property |
| HTTP latency, no SSE | 60/60 `200` in **22–133 ms** | device |
| HTTP latency, one SSE tab | **55 of 60** timed out at 5 s; survivors 6–7 s | device |
| After the SSE fix | **150/150 `200`**, 53 ms min / 67 ms p50, 92 frames in the same window | device |
| `/api/parameters?filter=all` | ~19 KB (3 × ~19 KB allocations → `abort()` in C++); 96 parameters; 32 KB response cap | ADR-0002 + device |

## Heater and carrier

| Metric | Value | How measured |
|---|---|---|
| Interrupt watchdog | **300 ms** | `components/esp_system/int_wdt.c` |
| LEDC spin at 1 Hz | up to **1 s** with interrupts masked | spin + 1 s period |
| Reachable duty resolutions at 1 Hz from an 80 MHz APB clock | **17, 18, 19, 20 only** — 16 bits overflows the max divider `0x3FFFF` | `div_param = ((src_clk<<8) + f·p/2)/(f·p)` |
| Chosen | **1 Hz / 17 bits**, `div_param = 156250`, period exactly **80 000 000 APB clocks = 1.000 000 s** | const-assert |
| Duty step at 17 bits / 1 Hz | **7.63 µs** | arithmetic |
| Narrowest pulse requested | **10 ms** — one whole C++ quantisation step | `on_fraction` grid |
| C++ ISR rate vs switching rate | **100 ticks/s**, but **0 or 2** contactor level changes/s | walking `isr.h:96-118` |
| Contactor min on/off, realised pin frequency and duty | **unmeasured** — needs a scope, a dummy load and the boiler disconnected | — |

At `Resolution::Bits20` full power becomes indistinguishable from disabled, because ESP-IDF's own
comment notes 100 % duty is unreachable when the timer selects maximum resolution. 17 bits is chosen
as the *coarsest* that works, which leaves the most margin against divider arithmetic being wrong.

## Sensors and buses

| Metric | Value | How measured |
|---|---|---|
| Fitted probe | **DS18B20**, family `0x28`, ROM `2869...af41` | device, 2026-09-28 |
| DS18B20 conversion | 750 ms at 12-bit, **375 ms at 11-bit** — the 400 ms tick requires 11 bits | datasheet |
| DS18B20 power-on scratchpad | **85 °C** placeholder for "no conversion performed yet" | datasheet |
| `DEVICE_DISCONNECTED_RAW` | −7040 = **exactly −55 °C** in 1/128 units — a range check, not a sentinel | arithmetic |
| 1-Wire tightest slot | **13 µs against a 15 µs window = 2 µs of slack**; `t_RST` 480 µs **at the limit** | datasheet + const-assert |
| ZACwire sampling | **7 µs** burst poll ≈ **143 kHz** (app note wants ≥128 kHz), ~40 000 polls/s, **~3 % duty** | arithmetic |
| ZACwire device path | **written, never brought up** — the pin carries 1-Wire traffic from the fitted DS18B20 | — |
| TSIC-306 protocol | 2048 codes walked by simulation; **a green run is not evidence a TSIC-306 works** | simulation |
| ABP2 frame | **12 bytes**; the C++ reads 7 and mixes the second word's status byte into the count | `drivers` port |
| ABP2 read cadence | `delay(10)` every 50 ms in C++ | `pressureSensor.h:35` |

## Network and storage

| Metric | Value | How measured |
|---|---|---|
| HTTP routes | 23 used of a 32 `max_uri_handlers` ceiling; `max_open_sockets` 4 → 5 | device |
| SSE cap | 2 clients, 4 mailbox slots, 50 ms broadcaster poll, 15 s keepalive | sized against 320 KB heap + NAT |
| MQTT | 1024 B buffer, 10 ms budget per iteration (2.5 % of the tick), 5 s interval | `MQTTManager.h:259` |
| Telnet log ring | 16 × 304 B ≈ **5 KB**, down from 64 × 576 B = 37 KB (**12 % of RAM**) | ADR-0002 |
| Heap-shed floor | **30 000 B** | ADR-0002 |
| Config blob | **2 077 B** serialised vs the recovered oracle's logged **2 071 B** for a 98-key schema | both |
| C++ `loadAll` on a healthy machine | reports **41 of 96** parameters loaded | device |

The 6-byte blob delta is treated as corroboration that the parameter shape is right. It is not proof.

## Tests

| Metric | Value |
|---|---|
| Host tests | 900+ |
| Device tests, on hardware | **111**, all registered and passing the audit |
| State machine table | 18 states × 46 events × 5 flavours = **4 140 pairs**, every pair reaching a named verdict |
| PID parity | against the **unmodified `PID_v1.cpp`**, compiled and replayed — scenarios A–C bit-identical, max abs Δ 0.0; D keeps the C++'s `NaN` on purpose |
| Display parity | against the **actual U8g2 tree the firmware links**, 11 scenarios, **zero differing pixels**, with an anti-trivial-pass guard (`total_ink > 10 000` lit pixels) |
| Parity scenarios | 17 covering S1–S11; 12 would energise actuators and run `dry_run` |
| Scenarios without a C++ baseline | **13 `BASELINE-MISSING`, exit 2** — nothing fabricated |
| PID on the board at a 7 K error | **473.9 ms of a 1000 ms window = 47.4 %**, P=449.5 I=8.6 D=15.7 |
| Display on the board | `present=true frames=125 failed=0` in 60 s = 2.08 Hz against a 100 ms target |
| ISR on the board | 42 ticks per 420 ms = **exactly 100 Hz**, 3108 ticks, 0 panics, heater never energised |

The 47.4 % figure is the headline proof that the port controls the heater rather than merely driving
it — the C++'s `setHeaterDuty` was a no-op and every duty was 100 %.

## Reproducing

`just size`, `just size-check`, `just test`, `just test-esp32`, `just test-audit`, and
`scripts/sse-starvation-check.py` for the SSE regression, which exits non-zero on starvation and
**exit 2 `INCONCLUSIVE`** if the client saw no frames — a pass with no traffic would prove nothing.
