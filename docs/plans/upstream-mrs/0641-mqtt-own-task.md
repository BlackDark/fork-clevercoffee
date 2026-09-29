---
title: MQTT weak-link — ungate RSSI, cap socket timeout, disable WiFi PS; dedicated task only if Phase 1 fails on device
upstream_pr: 641
upstream_repo: rancilio-pid/clevercoffee
upstream_url: https://github.com/rancilio-pid/clevercoffee/pull/641
category: take
advise: include
stake: Weak-RSSI MQTT telemetry is real on metal-body machines; a FreeRTOS MQTT task is not the first fix.
effort: S
risk: low
priority: 5
status: planned
created: 2026-09-11
tags:
  - upstream-mr
  - plan
---

# MQTT weak-link (upstream #641)

## Verdict

**Include, split into phases. Ship Phase 1. Do not start Phase 2 unless Phase 1 fails on a device at ≤ −75 dBm.**

Phase 1 (S, low risk) is the actual take: ungate the publish RSSI check, cap `PubSubClient` socket timeout to 3 s, disable WiFi modem sleep after STA connect/reconnect.

Phase 2 (L, high risk) is a dedicated FreeRTOS MQTT task. Upstream needed it because `mqtt.connect()` and full-cycle publish lived in `loopPid()` with a 15 s default timeout. The fork already differs enough that the task is not justified until Phase 1 is measured insufficient.

A full task is **not** justified as the opening move versus (a) ungate RSSI (b) cap socket timeout (c) `WiFi.setSleep(false)`. Upstream’s own “gate gone, still in the loop, brew timer stuck 15 s” measurement used power-save ON and the 15 s default — not the three-line combination.

## Upstream

Four commits (PR body describes three; the WDT cap landed as a fourth):

| Commit | What | Why they did it |
|---|---|---|
| 1 | MQTT FreeRTOS task pinned to core 0, 8 KiB stack, prio 1. Snapshots for `temperature` / `pidOutput` / `currBrewTime`. Incoming commands via 10-deep queue. `requestMqttPublish()` from the web handler. Drop brew-guard on `checkMQTT()`. Drop blocking `checkMQTT()` / discovery from `setup()`. | `loopPid()` stalled on `mqtt.connect()` (default 15 s). `writeSysParamsToMQTT()` called from loop **and** AsyncWebServer (PubSubClient not thread-safe; static iterators). |
| 2 | `WiFi.setSleep(false)` after connect and after reconnect | `WIFI_PS_MIN_MODEM` → 446 ms avg RTT at −85 dBm (router 4 ms). Power save off → 50 ms avg. Latency, not loss. Root cause of publishes hitting socket timeout. ~25 mA. |
| 3 | Drop 10 ms time budget / static iterators; publish a full cycle back-to-back | Budget existed only to keep work off the loop. Chunking every 100 ms defeated Nagle (5 TCP segments → 22). |
| 4 | `setSocketTimeout(3)` + `static_assert` vs `CONFIG_ESP_TASK_WDT_TIMEOUT_S`; `setKeepAlive(30)` | Task on core 0: `PubSubClient::connect()` busy-waits (knolleary/pubsubclient#670). `yield()` does not run the idle task. 8 s timeout starved core-0 idle past the 5 s TWDT → panic. |

Measured win: Silvia E at −85 to −88 dBm, 0 MQTT dropouts overnight vs 14 the night before.

They treat commit 2 as independently splittable. Agree.

## Current fork

MQTT lives in `MQTTManager`, driven from the Arduino loop on **core 1**:

```485:511:src/core/LoopManager.cpp
void LoopManager::updateNetwork() {
    // ...
        mqttManager->checkConnection();
        mqttManager->loop();
        if (mqttManager->isEnabled() && mqttManager->isConnected()) {
            mqttManager->setUpdateRunning(false);
            if (systemContext_.cleverCoffeeWiFiManager() &&
                systemContext_.cleverCoffeeWiFiManager()->getSignalStrength() > 1) {
                bool displayBufferNotReady = !systemContext_.uiCoordinator().isDisplayBufferReady();
                if (displayBufferNotReady) {
                    mqttManager->writeSysParamsToMQTT(true);
                }
            }
        }
```

`getSignalStrength()` returns **bars 0–4**, not dBm. Bars > 1 means RSSI ≥ −75 dBm. Bars 0 is < −80 dBm (typical in-machine).

```320:344:src/network/CleverCoffeeWiFiManager.cpp
int CleverCoffeeWiFiManager::getSignalStrength() {
    // ...
    if (rssi >= -50) return 4;
    else if (rssi < -50 && rssi >= -65) return 3;
    else if (rssi < -65 && rssi >= -75) return 2;
    else if (rssi < -75 && rssi >= -80) return 1;
    return 0;
}
```

Fork vs upstream, mapped to their three problems:

| Upstream problem | Fork today |
|---|---|
| MQTT never **connects** below −75 dBm (`checkMQTT()` gated) | **Already untrue.** `checkConnection()` always runs. Only `writeSysParamsToMQTT` is gated. Weak-RSSI machines connect but publish no telemetry. OLED `MQTT!` at bars ≤ 1 (`DisplayWidgets.h`) is UX, not a connect gate. |
| `writeSysParamsToMQTT` from two tasks | **Does not exist.** Only `LoopManager::updateNetwork()` calls it. Web save does not publish. Member iterators (`publishPhase_`, `mqttVarsIt_`) are single-task. |
| Blocking `connect()` in the control loop | **True.** `mqttClient_.connect()` in `MQTTManager::checkConnection()` (`MQTTManager.cpp` ~169). PubSubClient default socket timeout 15 s. No `setSocketTimeout`. No `xTaskCreate` anywhere. |
| Brew-timer / PID stall | `checkConnection()` already returns early while `brewHandler().isBrewActive()`. Publish during brew still runs if bars > 1. Heater PWM ISR (`include/clevercoffee/isr.h`) keeps toggling the last `pidOutput`; temperature sampling and PID compute are loop-bound and freeze for the stall. |
| WiFi power save | **Never disabled.** No `WiFi.setSleep`. Reconnect is `CleverCoffee::Network::reconnectStaWithHostname`. |
| Discovery at boot | Not in `setup()`. 300 s timer (`HASSIO_DISCOVERY_INTERVAL_MS`) on the loop via `sendHASSIODiscoveryMsg()`. |

Other fork constraints Phase 2 must respect:

- **ADR-0002**: ~320 KB RAM; AsyncWebServer already a FreeRTOS task; Logger ring is MPSC-atomic (safe to *produce* logs from another task; `flushRingBuffer()` stays main-loop only). Heap shed at 30 KB free.
- **`CONFIG_ASYNC_TCP_RUNNING_CORE=1`** (`platformio.ini`) — AsyncTCP on core 1 at prio 10. Upstream assumed AsyncTCP unpinned (`RUNNING_CORE=-1`).
- RetryPolicy + CircuitBreaker already wrap MQTT reconnect (better than upstream’s reconnect counter).
- `assignParameter()` writes `Config` / state-machine flags from `mqttClient_.loop()` → `messageCallback`. Today that is the main loop. A task must not call it in-place.
- Sensor lambdas in `SystemInitializer::registerMQTTSensors()` read `ProcessController` doubles (`temperature`, `pidOutput`, brew time). Not atomic across cores.

## Need it?

Outcome yes; their MQTT task no, not first. Phase 1 is the three behaviors in the Plan. Escalate to Phase 2 only on device evidence after that (brew/PID stall > ~100 ms, overnight dropouts at ≤ −80 dBm with PS off, or temp-sample gaps). Phase 2 wraps `MQTTManager`; first `xTaskCreate` in the fork.

## Plan

### Phase 1 — surgical (do this)

Completion of Phase 1: MQTT publishes at bars 0–1; `connect()`/`publish()` cannot block longer than 3 s; power save is off whenever STA is connected; OLED bars/`MQTT!` unchanged; native tests + firmware build green; no new FreeRTOS task.

#### 1. Ungate publish RSSI

File: `src/core/LoopManager.cpp` (`updateNetwork`).

Remove the `getSignalStrength() > 1` conjunct. Keep `isEnabled()`, `isConnected()`, and the display-buffer-not-ready guard.

Done when: `writeSysParamsToMQTT` is reachable at any connected RSSI. `DisplayWidgets.h` still uses bars for the antenna glyph and `MQTT!`.

#### 2. Cap PubSubClient timeouts

Files: `src/network/MQTTManager.cpp` (`initializeClient()` — ctor, before any `connect()`), `include/clevercoffee/network/MQTTManager.h` (named constant).

```cpp
static constexpr int kMqttSocketTimeoutS = 3;
mqttClient_.setSocketTimeout(kMqttSocketTimeoutS);
mqttClient_.setKeepAlive(30);
```

3 s, not 8 s: even on the loop, 8 s is a user-visible brew/PID freeze if the brew skip is ever bypassed; it is also the value that panicked upstream on core 0. Do not raise it.

Done when: the only socket timeout in the MQTT path is this constant, applied before any `connect()`.

#### 3. Disable WiFi power save

Files: `src/network/CleverCoffeeWiFiManager.cpp`.

`reconnectStaWithHostname` / `WiFi.begin()` is **async**. The immediate `WL_CONNECTED` check after it often misses. Call `WiFi.setSleep(false)` (`WIFI_PS_NONE`, not configurable):

1. `handleSuccessfulConnection()` — first STA association.
2. Rising edge of `WL_CONNECTED` in `checkAndMaintainConnection()` (`wifiConnectedHandled` false→true). That is where delayed reconnects land.

A one-liner next to `WiFi.begin()` in `WiFiStaConnect.h` is **not** enough.

Done when: sleep is cleared on first connect and on every reconnect that later reports connected.

Leave untouched in Phase 1: brew skip on `checkConnection()` (3 s reconnect mid-shot still jumps the timer); 10 ms publish budget; `mqttUpdateRunning_`.

#### Phase 1 files (exhaustive — three behaviors)

| File | Change |
|---|---|
| `src/core/LoopManager.cpp` | Drop RSSI gate on publish |
| `src/network/MQTTManager.cpp` | `setSocketTimeout(3)`, `setKeepAlive(30)` in `initializeClient()` (ctor, before any `connect()`) |
| `include/clevercoffee/network/MQTTManager.h` | timeout/keepalive constants |
| `src/network/CleverCoffeeWiFiManager.cpp` | `WiFi.setSleep(false)` first-connect + `WL_CONNECTED` rising edge |
| `docs/integration-tests.md` | weak-RSSI / unreachable-broker MQTT checks |

Leave untouched: iterators/time budget, brew skip, discovery timer, `assignParameter`, `IMQTTManager`, web server, `DisplayWidgets.h`, `getSignalStrength()` thresholds.

Optional later (not Phase 1): extract `rssiToWifiBars` + native bar table. OLED scale is unchanged; MQTT no longer consults it. Do not block Phase 1 on that extract.

### Phase 2 — MQTT task (only if Phase 1 fails on device)

Completion of Phase 2: every `mqttClient_` call runs on one MQTT task; `connect()` cannot stall PID compute or the brew timer; incoming parameter writes apply on the main loop; heap/core/WDT constraints below are met; Logger flush stays on the main loop.

#### Placement

Keep `MQTTManager` as the owner. Add `startTask()` / `stopTask()` there. `LoopManager::updateNetwork()` becomes: drain command queue + snapshot telemetry. Do not recreate upstream `mqtt.h` + `mqttTask` in `main.cpp`.

#### Core, stack, priority

| | Value | Reason |
|---|---|---|
| Core | **0** | Arduino loop is core 1. AsyncTCP is already core 1 prio 10. MQTT on core 1 would busy-wait on the same core as PID. Core 0 is LWIP/WiFi — correct home for socket I/O. |
| Stack | **8192** | Match upstream; discovery uses `JsonDocument` + `String`. Measure high-water with `uxTaskGetStackHighWaterMark` on device before shrinking. |
| Priority | **1** | Match loop. Must stay below AsyncTCP (10). Must **not** outrank idle in a way we then try to `yield()` through — `connect()` still busy-waits. |
| Period | `vTaskDelay(pdMS_TO_TICKS(100))` | Match upstream. |

`xTaskCreatePinnedToCore` failure: log ERROR, leave MQTT on the loop path or disabled. Fail closed. No boot hang.

#### WDT / `connect()`

`PubSubClient::connect()` busy-waits without sleeping (pubsubclient#670). On core 0 that starves the idle task (`CONFIG_ESP_TASK_WDT_CHECK_IDLE_TASK_CPU0`, 5 s, panic). Upstream core-dumped exactly this.

Keep socket timeout at **3 s** with:

```cpp
static_assert(kMqttSocketTimeoutS < CONFIG_ESP_TASK_WDT_TIMEOUT_S,
              "MQTT socket timeout must stay below the core 0 task watchdog");
```

`yield()` is not a substitute. Do not raise the timeout. Do not call `connect()` from `setup()` / `SystemInitializer`.

Heater ISR path: ISR reads `pidOutput` every 10 ms independently of the loop. Phase 2 exists so the **loop** keeps sampling temperature and computing PID. The task absorbing a 12 s TCP stall is the point. PID compute and brew-time increment must not wait on `mqttClient_`.

#### Thread-safety (hard rule)

PubSubClient is not thread-safe. **All** `mqttClient_` methods (`connect`, `loop`, `publish`, `subscribe`, `beginPublish`, `connected`, `state`, `setServer`, `setCallback`, …) run on the MQTT task after `startTask()`.

| Direction | Mechanism |
|---|---|
| Outgoing telemetry | Main loop writes `volatile float` snapshots for temperature, heater power, brew time (doubles are two 32-bit accesses). Sensor lambdas in `registerMQTTSensors` read those snapshots, not `ProcessController` live doubles. Other sensors that are already 32-bit/atomic can stay. |
| Incoming commands | `messageCallback` `xQueueSend` (`timeout 0`) of `{param[120], double}`. Main loop `processMqttCommands()` → existing `assignParameter()`. Queue depth 10; drop + WARNING on full. `assignParameter` / `Config` / state flags stay core 1. |
| Web-triggered publish | Not a current caller. If added: `requestMqttPublish()` = clear rate-limit stamp. Web handler must not call `writeSysParamsToMQTT`. |
| Discovery | MQTT task sends on first `connected()` edge, retries via existing failed flag. Remove the 300 s loop timer **or** make the timer only set a flag the task observes. `sendHASSIODiscoveryMsg()` is a long blocking burst — it must not return to the loop. |

Drop the 10 ms time budget once publish is off the loop (full cycle, Nagle can coalesce). Drop the brew skip on `checkConnection` only after the task owns `connect()`.

#### Logger (ADR-0002)

Produce logs from the MQTT task (`LOG`/`LOGF` OK — MPSC ring). Do not call `Logger::update()` / `flushRingBuffer()` from the MQTT task. Do not enlarge the ring.

#### Heap budget

Static cost to count before merging: 8 KiB task stack + queue (`10 * sizeof(MqttCommand)` ≈ 1.3 KiB) + 3× `volatile float`. Discovery `JsonDocument`s stay temporary on the MQTT stack/heap, not an extra static. After the change: `/api/parameters?filter=all` still returns full JSON with telnet connected (ADR-0002 checklist).

#### Phase 2 files (if reached)

| File | Change |
|---|---|
| `include/clevercoffee/network/MQTTManager.h` | task handle, queue, snapshots, `requestMqttPublish`, `processMqttCommands`; drop time-budget iterators |
| `src/network/MQTTManager.cpp` | `mqttTask`, move `checkConnection`/`loop`/`writeSysParamsToMQTT`/discovery into it; queue in callback |
| `include/clevercoffee/network/IMQTTManager.h` | `processMqttCommands` / `requestMqttPublish` if LoopManager talks via the interface |
| `src/core/LoopManager.cpp` | drain commands + snapshot; stop calling `checkConnection`/`writeSysParamsToMQTT`/`loop` |
| `src/core/SystemInitializer.cpp` | `startTask()` after `setup()`; sensor lambdas read snapshots |
| `include/clevercoffee/utils/SystemUtils.h` | discovery timer becomes a request flag, or is deleted |
| `test/PubSubClient.h` | `setSocketTimeout` / `setKeepAlive` if real `MQTTManager.cpp` is compiled natively |
| `docs/adr/0002-...` | short note: MQTT task on core 0, stack 8 KiB, Logger produce-only |

## Do not copy

Rewrite `MQTTManager` in the fork’s types. Do not paste upstream `src/mqtt.h` / `mqttTask` in `src/main.cpp` / `src/embeddedWebserver.h`.

Positive targets:

- Phase 1: three behaviors (ungate, timeout, power save) only. No `WifiRssiBars.h` required.
- Phase 2: one task **inside** `MQTTManager`, core 0, 8 KiB, timeout 3 s, all `mqttClient_` calls on that task, commands queued to the loop, float snapshots for 64-bit sensors.
- OLED `getSignalStrength()` remains bars for the antenna and `MQTT!`.
- `CONFIG_ASYNC_TCP_RUNNING_CORE=1` stays. MQTT task does not move to core 1.

## Tests / verification

### Native (`pio test -e native_test`)

Phase 1:

- No new native test required. MQTTManager.cpp is not compiled under `native_test` today (`HandlerTestStubs`). If that changes, stub `setSocketTimeout` / `setKeepAlive` (and the 7-arg `connect` with will) in `test/PubSubClient.h`.
- Optional: `rssiToWifiBars` table (−50→4 … −85→0). Proves OLED bars, not the MQTT gate (gate is gone). Skip unless extracting the helper.
- Existing `test_wifi_sta_hostname` still passes. Add `setSleep` assertions only if the stub grows and the call site is the tested helper.

Phase 2 (only then):

- Queue drop-when-full (host fake queue or a thin wrapper).
- `assignParameter` is not invoked from the callback in the test double — commands surface via `processMqttCommands`.
- Do not try to run FreeRTOS/`xTaskCreate` under `native_test`.

### Device (required for Phase 1 sign-off; required again if Phase 2)

On a machine that sits at ≤ −75 dBm inside the boiler shell (or with AP far enough to hold that RSSI):

1. Boot with MQTT enabled. Log shows reconnect attempts even at bars 0 (already true) **and** published topics (`status=online`, temperature) at that RSSI.
2. Unreachable broker: loop time stays bounded (~3 s worst case on a reconnect attempt, not 15 s). Brew timer does not jump. PID still samples (slow-loop warning in `LoopManager` may fire once; it must not hang).
3. Power save: ping RTT from a LAN host to the ESP drops from hundreds of ms toward tens of ms after connect; value survives a WiFi reconnect.
4. HA: discovery still appears (300 s timer in Phase 1 is acceptable). Phase 2: entities within ~15 s of broker connect.
5. ADR-0002: telnet logger + `/api/parameters?filter=all` concurrent, no OOM.
6. Overnight: MQTT session holds; compare dropout count to pre-change.

Add those bullets to `docs/integration-tests.md` when implementing.

### Build gates before commit

`pio run --target format -e esp32_usb -s`  
`pio run -e esp32_usb -s`  
`pio test -e native_test`

## Risks

| Risk | Phase | Mitigation |
|---|---|---|
| Ungating RSSI without timeout cap → 15 s loop stall (upstream measured this) | 1 | Ship timeout + power save in the **same** Phase 1 PR. Do not land the gate deletion alone. |
| 3 s stall still jumps brew timer | 1 | Keep brew skip on `checkConnection`. Re-measure; escalate to Phase 2 if publish() still blocks mid-shot. |
| `setSleep(false)` forgotten after reconnect | 1 | First-connect + `WL_CONNECTED` rising edge (begin is async; immediate check after `reconnectStaWithHostname` often misses). |
| Core 0 TWDT panic from `connect()` busy-wait | 2 | Timeout 3 s + `static_assert`; never `yield()` as the fix; never pin this task to core 1. |
| PubSubClient used from two tasks | 2 | All `mqttClient_` on the MQTT task. Web/loop only set flags or snapshots. |
| `assignParameter` / `Config` races | 2 | Queue to core 1. |
| Torn doubles in sensor lambdas | 2 | Float snapshots written on the loop. |
| 8 KiB stack + discovery JsonDocument vs ~95 KB free heap | 2 | Count static RAM; fail closed if task/queue alloc fails; re-run parameters+telnet. |
| Logger consumer from MQTT task | 2 | Produce-only; flush remains `LoopManager`/`Logger::update()`. |
| Heater ISR stale duty if Phase 1 still stalls PID compute | 1 | Bound stall to 3 s; ISR keeps last window. Phase 2 if 3 s thermal drift is visible. |
