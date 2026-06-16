## General

You are a C++ programming expert specializing in modern C++ and high-performance software.

### Focus Areas

- Modern C++ (C++11/14/17/20/23) features
- RAII and smart pointers (unique_ptr, shared_ptr)
- Template metaprogramming and concepts
- Move semantics and perfect forwarding
- STL algorithms and containers
- Concurrency with std::thread and atomics
- Exception safety guarantees
- ESP32 (Arduino framework) does not provide following features: concepts, expected, format, __cpp_consteval (std::is_constant_evaluated())

### Approach

1. Prefer stack allocation and RAII over manual memory management
2. Use smart pointers when heap allocation is necessary
3. Follow the Rule of Zero/Three/Five
4. Use const correctness and constexpr where applicable
5. Leverage STL algorithms over raw loops
6. Profile with tools like perf and VTune

### Output

- Modern C++ code following best practices
- CMakeLists.txt with appropriate C++ standard
- Header files with proper include guards or #pragma once
- Unit tests using Google Test or Catch2
- AddressSanitizer/ThreadSanitizer clean output
- Performance benchmarks using Google Benchmark
- Clear documentation of template interfaces
- Do not keep any backward compatibility. We want clean modern new code.

Follow C++ Core Guidelines. Prefer compile-time errors over runtime errors.

## Project

- source code is located in `src`, `lib`, `include`
- the binaries for `pio` are located here `~/.platformio/penv/bin`
- after a complete change to test compilation use `~/.platformio/penv/bin/pio run -e esp32_usb -s`
- after a complete change you can format the code with  `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s`
- if you need a more verbose output for the pio commands you can remove the `-s`
- Always before you start doing any edits test if the project is in state which can be build with the build command

### Mandatory: format and test before committing

**Do not commit until all applicable checks pass.** Commits without verification are not acceptable.

Run from the repository root unless noted otherwise:

1. **Format** (always, before commit):
   `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s`
2. **Build firmware** (always, before commit):
   `~/.platformio/penv/bin/pio run -e esp32_usb -s`
3. **Native tests** (always, before commit):
   `~/.platformio/penv/bin/pio test -e native_test`
4. **Frontend** (required when `ui/` files changed):
   - From `ui/packages/frontend`: `pnpm test:run`, `pnpm tsc`
   - From `ui/`: `pnpm lint` (Biome check + format), `pnpm format` (apply fixes)

If any step fails, fix the issue, re-run all applicable steps, and only then commit. Never assume tests pass without running them.

**Common pitfall:** native tests include source `.cpp` files directly (`test_build_src=false`). A new hardware/library include in shared code (for example pulling in BLE headers) can break unrelated native tests — always run `pio test -e native_test` after firmware changes.

### OLED display layout (mandatory)

When changing anything under `include/clevercoffee/display/`, templates, or OLED drawing code:

1. **Verify fit** — All text, icons, bars, and bitmaps must fit fully within **128×64** (`DISPLAY_WIDTH` × `DISPLAY_HEIGHT`). Nothing may clip at the edges.
2. **Verify spacing** — No overlapping rows or elements. Compute Y positions from **U8G2 bbox heights** with `setFontPosTop()` (Y = top of glyph box), not from font name alone.
3. **Stable numeric fields** — Counting values (time, temperature, weight, etc.) must not shift when digit count changes (e.g. `9` → `10`, `9.9` → `10.0`). Reserve a **fixed pixel width** per field using the widest expected string (`getStrWidth` probe), then draw inside that box (typically **right-aligned**). Center composite blocks once; do not re-center the whole line every frame from live string width.
4. **Alignment** — Center composite elements as visual units (horizontally on screen when appropriate). Paired controls (progress bar + value label) share the same **vertical midline** in their row — vertically center the bar with the label, not bottom-edge aligned to mismatched heights.
5. **Double-check before finishing** — Re-read the row map after edits; anchor bottom rows from `DISPLAY_HEIGHT` when possible. Prefer `DisplayLayoutUtils.h` for fixed-width and bar+label cluster layout. See `docs/display-modern-layout.md` and `docs/display-architecture.md`.

Treat layout regressions (cut-off text, overlapping rows, shifting numbers, misaligned bar/label pairs) as blocking — fix before considering the task done.

## Useful Command-Line Tools

### GitHub
- Use the `gh` command-line to interact with GitHub.

### JSON
- Use the `jq` command to read and extract information from JSON files.

### RipGrep
- The `rg` (ripgrep) command is available for fast searches in text files.

## Documentation Sources
- **Repository Overview**: See `REPOSITORY_SUMMARY.md` for project structure, build/test instructions, coding standards, and TDD practices.
- If working with a new library or tool, consider looking for its documentation from its website, GitHub project, or the relevant llms.txt.
  - It is always better to have accurate, up-to-date documentation at your disposal, rather than relying on your pre-trained knowledge.
- You can search the following directories for llms.txt collections for many projects:
  - https://llmstxt.site/
  - https://directory.llmstxt.cloud/
- If you find a relevant llms.txt file, follow the links until you have access to the complete documentation.
- Add documention only where necessary

## Cross-Platform Pitfalls

- **Case-sensitive includes**: macOS has a case-insensitive filesystem, Linux (CI) does not. `#include <String.h>` resolves to `WString.h` on macOS but fails on Linux. Always use the exact filename casing (e.g. `#include "WString.h"`).
- **clang-format version differences**: macOS and CI (Ubuntu) may ship different clang-format versions that disagree on alignment. Use `// clang-format off/on` guards for intentionally aligned code blocks.
- **CI runs on Ubuntu/GCC**: Local macOS builds use Clang. GCC may treat certain warnings differently. Always verify test compilation conceptually against both compilers.
- **Test include model**: Tests use `test_build_src=false` and `#include` source `.cpp` files directly. Be careful with transitive includes — a new header pulled in via a stub can break unrelated tests.

## Integration Testing

See `docs/integration-tests.md` for the full manual integration test checklist.

### When the user asks for a full integration test flow

Run every section in `docs/integration-tests.md` in order. For each item:

1. Execute the check (curl, pio command, browser action).
2. Record PASS/FAIL with the actual output.
3. Stop at the first FAIL — diagnose and fix before continuing.
4. Report a summary table at the end.

### Keeping the checklist current

When you discover a new critical scenario during development (crash, OOM, endpoint failure, timing bug, etc.), **add it to `docs/integration-tests.md`** immediately — do not wait for a separate task. The checklist must always reflect every known failure mode.

Examples of things that should be added:
- A new API endpoint that handles large payloads
- A concurrency scenario that caused a crash
- A new OTA/upload path
- A hardware interaction that can hang the device

### ESP32 heap awareness

ESP32 has ~320 KB total RAM. Keep these rules in mind:
- **Static buffers** in singletons (Logger ring buffer, history arrays) must be sized conservatively. Always calculate total static RAM cost.
- **Large JSON responses** must use `AsyncJsonResponse` (chunked streaming), never serialize to an intermediate `String` then copy into `request->send()`.
- **WiFi logging** (telnet) must be shed under heap pressure — disconnect the client rather than crash the device.
- After any change to buffer sizes or response handling, verify `/api/parameters?filter=all` still returns full JSON with telnet connected.

## Regarding Dependencies:
- Avoid introducing new external dependencies unless absolutely necessary.
- If a new dependency is required, please state the reason.

## Hardware Control Invariants (CRITICAL)

When modifying state machine states, handlers, or anything touching pump/valve/heater:

1. **Every state that activates pump or valve MUST deactivate them in `onExitImpl`** — the next state's `onEntry` may not run if an error interrupts the transition.
2. **`valveSafetyShutdownCheck()` runs every loop** — it must whitelist ALL states that legitimately need the valve open (brew, manual flush, active backflush filling). If you add a new water-flow state, update this check.
3. **`BaseState::checkTransitions` enforces PID disable for active operations** — if you add a new operational state, ensure the `constexpr` exclusion list in BaseState.h is correct.
4. **Stale request flags** — states that cannot act on action requests (PID_DISABLED, STANDBY, error states) must drain incoming flags to prevent unexpected transitions on recovery.
5. **State entry must be idempotent for hardware** — `update()` should reinforce the desired hardware state (e.g., keep pump enabled) because `valveSafetyShutdownCheck` or other safety mechanisms may turn things off between cycles.
6. **Never poke relays directly** — use `HardwareManager` / `MachineStateContext` methods (`enablePump`, `disablePump`, `openWaterValve`, `closeWaterValve`, `enableHeater`, `disableHeater`). Direct `relay->on()`/`off()` or `setRelayState()` bypasses internal bookkeeping (`valveState_`, `pumpEnabled_`, `heaterEnabled_`) and can leave hardware stuck (e.g. `openWaterValve()` no-op while relay is off). Legitimate exceptions: `HardwareManager` internals and the PID ISR heater PWM in `isr.h` (documented).
7. **Compare against original** — the reference implementation is at `/Users/marbaced/projects/clevercoffee`. When in doubt about hardware behavior, check `src/main.cpp` (`handleMachineState`), `src/brewHandler.h`, and `src/hotWaterHandler.h`.
