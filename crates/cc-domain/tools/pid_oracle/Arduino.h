/*
 * Minimal Arduino shim for the native PID parity oracle.
 *
 * PID_v1.cpp includes <Arduino.h> and uses exactly one thing from it: `millis()`.
 * The oracle drives that clock by hand, so the shim exposes a settable virtual
 * clock instead of a real one. Nothing else from Arduino is used.
 *
 * The virtual clock is a `uint32_t` on purpose. `millis()` returns `unsigned
 * long`, which is 64-bit on a macOS/Linux host and 32-bit on the ESP32, and
 * PID_v1.cpp does unsigned arithmetic on it (`now - lastTime`, and
 * `lastTime = millis() - SampleTime` in the constructor). Keeping every value the
 * oracle feeds in below 2^32 makes the two widths agree:
 *
 *   * while `now >= lastTime` the subtraction is exact in both widths;
 *   * the constructor's `millis() - SampleTime` can go negative only when
 *     `millis() < SampleTime`, and the first `Compute()` then takes the
 *     "enough time has passed" branch under both widths.
 *
 * The wrap-around step in the scenario additionally pins the ESP32's real
 * 2^32 behaviour: the Rust port uses `u32::wrapping_sub`, and the oracle
 * exercises the same values.
 *
 * This shim exists ONLY for the host parity oracle in crates/cc-domain/tools.
 * It is never compiled into the firmware.
 */
#pragma once

#include <cstdint>

/* Virtual clock. Assign before each PID::Compute(). Values are masked to 32 bits. */
inline uint32_t g_oracle_millis = 0;

inline unsigned long millis() {
    return static_cast<unsigned long>(g_oracle_millis);
}
