---
title: WiFi reconnect must not latch offline
upstream_pr: 642
upstream_repo: rancilio-pid/clevercoffee
upstream_url: https://github.com/rancilio-pid/clevercoffee/pull/642
category: take
advise: include
stake: "Runtime reconnect exhaustion latches NetworkCoordinator offline forever, so MQTT stays dead after WiFi returns until reboot."
effort: S
risk: low
priority: 1
status: planned
created: 2026-09-11
tags:
  - upstream-mr
  - plan
---

## Verdict

**INCLUDE.** Hypothesis confirmed. Fork is **worse** than upstream: same irreversible latch, plus CircuitBreaker recovery is wasted. Not a cherry-pick — rewrite the maintain path only.

Two flags, one field: `Config::systemOfflineMode` (`system.offline_mode`) is intentional and must stay latched. `NetworkCoordinator::offlineMode_` is also set from reconnect/boot WiFi failure. `checkAndMaintainConnection()` returns immediately while that bit is set, so STA/MQTT never recover. Do not mix them.

## Upstream

`checkWifi()` in `src/main.cpp` called `initOfflineMode()` after `maxWifiReconnects` (5 × 10s ≈ 1 min). That function is startup-only: `offlineMode=true`, `mqtt_enabled=false`, `WiFi.softAP(...)`. Nothing clears it; `checkWifi()` and `ArduinoOTA.handle()` sit behind `!offlineMode`. Kitchen WiFi blip → MQTT + OTA dead until reboot; ping/web still work (AsyncTCP ignores the flag).

Fix (13/2): on exhaustion, **reset** `wifiReconnects=0` and `wifiConnectCounter=1`, log, wait. Startup `initOfflineMode()` in `wiFiSetup()`/`setup()` unchanged.

## Current fork

Verified in `src/network/CleverCoffeeWiFiManager.cpp` `checkAndMaintainConnection()`:

1. Line 209–212: `if (networkCoordinator_->isOfflineMode()) return;` — permanent skip.
2. Line 254–262: `!retryPolicy_->shouldRetry()` → `setOfflineMode(true)` then return.
3. Line 311–317: `circuitBreaker_->isOpen() && retryPolicy_->isMaxAttemptsReached()` → `setOfflineMode(true)` again.

`RetryPolicy(10s, 5min, 2x, max=5)` + `CircuitBreaker(5 failures, 60s OPEN, 30s half-open)` already pause. After 5 failed `reconnectStaWithHostname` calls, CB is OPEN (would HALF_OPEN after 60s) **but offline latch kills that**.

Also latches the same bit:

- `SystemInitializer::setupWiFi()` 845: `setupAndConnect` false → `setOfflineMode(true)` (router late at boot = never retry).
- `initializeNetworking()` 446–449: **keep** — `Config::systemOfflineMode`.
- `initOfflineMode()` in `include/clevercoffee/utils/SystemUtils.h` is **dead** (never called). Do not revive. Do not add SoftAP.

Consumers of the latch: `MQTTManager::checkConnection()` returns; `LoopManager::updateWebsite()` skips SSE; display WiFi widgets hide. `ArduinoOTA.handle()` in `LoopManager::updateNetwork()` is **not** gated (better than upstream). `LoopManager` 518 only `resetWifiReconnects()` when connected **and** not offline — cannot unstick.

## Need it?

Yes. Field bug: flaky STA → MQTT gone until reboot. Circuit breaker already is the pause; latch is the defect. Intentional `system.offline_mode` is a different path.

## Plan

No new header. Native cannot link `WiFiManager`; wrapping `retry.reset()` / a bool skip in `WifiMaintainPolicy.h` is a single-use abstraction. `RetryPolicy::reset` and CB OPEN are already tested.

1. **Wire maintain** — `src/network/CleverCoffeeWiFiManager.cpp` `checkAndMaintainConnection()`:
   - Skip reconnect iff `Config::getInstance().systemOfflineMode.get()`. Replace the 209–212 coordinator skip. Do **not** skip on runtime `isOfflineMode()`.
   - Delete both `setOfflineMode(true)` blocks (254–262 and 311–317).
   - `!shouldRetry()` → `retryPolicy_->reset(); return;` (no `WiFi.begin` this tick; never touch `NetworkCoordinator`).
   - Leave CB as-is: OPEN → existing `canAttempt` early return; HALF_OPEN after 60s starts the new round.
   - Keep `yield()`; add **no** `delay()`.
   - Update comment on `checkAndMaintainConnection` in `include/clevercoffee/network/CleverCoffeeWiFiManager.h` (remove “Offline mode activation after max attempts”).

2. **Boot fail must not share the latch** — `src/core/SystemInitializer.cpp` `setupWiFi()` 845: on `setupAndConnect` false, **do not** `setOfflineMode(true)`. OLED “No WiFi” stays. Let maintain retry. Keep 446–449 config path. Exception path 859: leave the coordinator latch (MQTT/widgets stay off); maintain skip is Config-only so STA may still retry.

3. **Do not clear config offline** — never `setOfflineMode(false)` from maintain/success. Success path already `retryPolicy_->reset()` + `circuitBreaker_->recordSuccess` + `resetWifiReconnects`.

4. **MQTT/OTA** — no MQTTManager changes. `isOfflineMode()` guard stays for **config** offline (and the exception latch). Do not wrap `ArduinoOTA.handle()` in offline checks. Do not start SoftAP.

5. **Verify** — `~/.platformio/penv/bin/pio run --target format -e esp32_usb -s` then `pio test -e native_test` then `pio run -e esp32_usb -s`.

Completion: zero `setOfflineMode(true)` in `CleverCoffeeWiFiManager.cpp`; config offline still skips reconnect; 5 failed reconnects leave `isOfflineMode()==false` and CB still pauses; no new header / `delay()` / `softAP` on this path.

## Do not copy

- Upstream `initOfflineMode()` / `WiFi.softAP` from runtime `checkWifi`.
- `mqtt_enabled = false` as a side effect of reconnect exhaustion.
- Gating `ArduinoOTA.handle()` on offline.
- Cherry-pick of `wifiReconnects` / `wifiConnectCounter` (fork uses RetryPolicy + CircuitBreaker).
- Resetting CircuitBreaker on exhaustion (that pause **is** the backoff).
- Clearing `system.offline_mode` or coordinator bit set from config.
- New deps, SoftAP portal on blip, changing CB thresholds.
- `WifiMaintainPolicy.h` (or any other skip/reset wrapper).

## Tests / verification

- Native (required): existing `test/test_utils/test_retry_policy` (`ShouldRetry` + `Reset`) and `test/test_utils/test_circuit_breaker` unchanged. No new native filter.
- Optional: `test/test_network_coordinator` — config-style `setOfflineMode(true)` stays true across `resetWifiReconnects()` (documents latch ≠ reconnect counter).
- Firmware: `pio run -e esp32_usb -s`.
- Device: STA down ≥5 reconnects (~minutes with 10s×2^n cap 5min) → logs retry/CB OPEN, **not** “entering offline mode”; restore AP → MQTT resumes without reboot; `system.offline_mode=true` → no reconnect, MQTT stays off.
- Add one item to `docs/integration-tests.md`: reconnect exhaustion must not latch offline; MQTT recovers when STA returns.

## Risks

- **Config vs runtime mix-up (medium if ignored):** using `isOfflineMode()` as the maintain skip after boot-fail still latches. Skip must be **config** (step 1–2).
- **Spin after retry reset:** `currentAttempt_==0` makes `canRetryNow` true immediately. Mitigated by returning that tick and by CB OPEN. Do not reset CB.
- **Control loop:** `reconnectStaWithHostname` is async + `yield()`. Do not add `delay()`. CB fail-fast is cheap.
- **Boot without AP:** machine stays STA-retrying instead of “offline product mode”. Correct unless user set `system.offline_mode`.
- **MQTT setup at boot with no WiFi:** `initializeMQTT` may run if boot no longer sets the latch; manager already allows later `checkConnection` retry — acceptable.
