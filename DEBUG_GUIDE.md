# Debug Guide: Diagnosing State Machine and ISR Execution

## Summary
The firmware has comprehensive debug logging added to help diagnose if the state machine is updating and if the ISR (heater relay control) is executing correctly.

## How to Test

### 1. Flash the Firmware
```bash
~/.platformio/penv/bin/pio run -e esp32_usb --target upload
```

### 2. Open Serial Monitor
```bash
screen /dev/ttyUSB0 115200
# or
miniterm.py /dev/ttyUSB0 115200
# or use PlatformIO's serial monitor
```

### 3. Expected Boot Output

You should see initialization messages:
```
[INFO] System initialization completed successfully
[INFO] LoopManager initialized successfully with centralized sensor timing
[INFO] ProcessController initialized successfully
[INFO] Handlers initialized successfully
[DEBUG] Setting global SystemContext at 0x3d844a20
[DEBUG] Global SystemContext set: ptr=0x3d844a20, valid=1
[DEBUG] Calling setupTiming()
[DEBUG] Calling enableTimer1() - ISR will now fire
[DEBUG] Timer enabled - ISR should be firing every 10ms
```

If you see this, the initialization was successful and the ISR should be firing.

## Key Metrics to Monitor (logged every 5 seconds)

### Example Good Output:
```
[INFO] LOOP STATUS: loops=200, ISR enabled=1, ISR calls=3000, relay_on=1500, relay_off=1500, temp=23.5°C, setpoint=90.0°C, pidOutput=500.0
```

### What Each Value Means:

| Metric | Good Value | Bad Value | Meaning |
|--------|-----------|-----------|---------|
| `ISR enabled` | 1 | 0 | Is ISR executing at all? Must be 1 |
| `ISR calls` | Increasing (50 per second expected) | Not increasing | ISR firing rate. Should increase by ~250 every 5 sec |
| `relay_on` | Increasing | Stays 0 or not increasing much | Heater turns on. Should increase during heating phase |
| `relay_off` | Increasing | Stays 0 or not increasing much | Heater turns off. PWM cycling needed |
| `temp` | Updates over time | Fixed value | Temperature sensor reading |
| `setpoint` | >0 (usually 90-95) | 0 or 20 | Target temperature set in config |
| `pidOutput` | 0-1000 | Fixed 0 | PID controller output (0=off, 1000=on) |

## Detailed Logging Levels

### To See All DEBUG Logs
Open a telnet connection to the ESP32 on port 23:
```bash
telnet esp32.local 23
# Then in the telnet session, you can change log level
# (check Logger.h for available log level commands)
```

Or in the web UI (if available), set log level to DEBUG.

### Expected Log Sequence (at DEBUG level):

```
[DEBUG] updateProcessControl: Entering with state=4, temp=23.5°C, setpoint=90.0°C, pidOutput=0.0
[DEBUG] updateProcessControl: After update: temp=23.5°C, setpoint=90.0°C, pidOutput=0.0, timer=15ms
[DEBUG] StateMachine::update() -> State: PID_MODE (4)
[DEBUG] StateMachine::update() -> State: PID_MODE (4)
[DEBUG] updateTemperatureSensor: Reading temperature
[DEBUG] LoopManager: Using UIManager path for display updates
```

When transitioning states, you should see:
```
[WARNING] State transition detected: 0 -> 4 (INIT_STATE -> PID_MODE)
```

## Troubleshooting Checklist

### Problem: ISR enabled=0 or ISR calls not increasing

**Diagnosis:** The ISR is not executing at all.

**Check:**
1. Is SystemContext being set before enableTimer1()?
   - Look for: `Global SystemContext set: ptr=0x..., valid=1`
   - If `valid=0`, the context pointer is invalid

2. Is the ISR returning early?
   - The ISR has safety checks (isr.h lines 20-28)
   - Both `timer` and `SystemContext` must be valid
   - Since we just added context check, timer is likely the issue

**Solution:**
- Check that `initTimer1()` is being called before `enableTimer1()`
- Look in serial output for any errors during phase 5 (Finalization)

### Problem: ISR calls increasing, but relay_on and relay_off not changing

**Diagnosis:** The ISR is executing but not controlling the relay.

**Check:**
1. Is the heater relay being found?
   - Look for any ERROR messages about relay
   - Check: `ctx->hardwareContext().heaterRelay()` in isr.h line 37

2. Is PID output correct?
   - Check `pidOutput` value in the log
   - If state is not PID_MODE, pidOutput will be 0
   - If state is PID_MODE, pidOutput should be between 0-1000

**Solution:**
- Verify state machine is in PID_MODE (state=4)
- Check ProcessController::updateProcessControl() is being called

### Problem: Temperature reading never changes

**Diagnosis:** The temperature sensor is not working or not being read.

**Check:**
1. Is the sensor coordinator reading the temperature?
   - Look for `[DEBUG] updateTemperatureSensor: Reading temperature`
   - Look for temperature update logs

2. Is there an I2C or Dallas wire error?
   - Look for ERROR messages about sensor communication

**Solution:**
- Check sensor hardware connections
- Verify sensor configuration in Config.h
- Check I2C address if using I2C temperature sensor

### Problem: State machine not updating (always shows same state)

**Diagnosis:** The state machine is stuck in one state.

**Check:**
1. Are you seeing the state update logs?
   - Look for: `[DEBUG] StateMachine::update() -> State: ...`
   - This should appear many times per second

2. Are state transitions happening?
   - Look for: `[WARNING] State transition detected: X -> Y`
   - These should appear when conditions change

**Solution:**
- Check if all required state conditions are met
- For INIT_STATE to PID_MODE: check if splash screen timeout has expired
- Check ProcessController is initializing correctly

## Real-Time Debugging Strategy

### Step 1: Verify ISR is executing (5 seconds)
```
ISR enabled=1 and ISR calls is increasing by ~250 every 5 seconds → ✅ ISR working
ISR enabled=0 or ISR calls not increasing → ❌ ISR not firing, CRITICAL BUG
```

### Step 2: Verify relay is toggling (if state is PID_MODE)
```
relay_on and relay_off both increasing → ✅ Relay PWM working
One or both not increasing → ❌ Relay not being controlled
```

### Step 3: Verify temperature reading (should update over time)
```
temp value changes slowly over time → ✅ Sensor working
temp stays fixed → ❌ Sensor not being read
```

### Step 4: Verify state machine updates (with DEBUG logging)
```
See "StateMachine::update()" logs every loop → ✅ State machine active
Don't see these logs → ❌ State machine not updating
```

### Step 5: Verify PID controller executes (with DEBUG logging)
```
See "updateProcessControl:" logs with changing pidOutput → ✅ PID working
pidOutput stays 0 or doesn't update → ❌ PID not executing
```

## ISR Counter Explanation

The ISR uses volatile counters to track execution without logging (since logging in ISR is dangerous):

```cpp
// In isr.h:
volatile bool isr_enabled = false;           // Set to true on first ISR call
volatile uint32_t isr_call_count = 0;        // Incremented every ISR call
volatile uint32_t isr_relay_on_count = 0;    // Incremented when relay turned on
volatile uint32_t isr_relay_off_count = 0;   // Incremented when relay turned off
```

**Expected rates:**
- ISR calls: ~100 per second (10ms timer)
- In 5 seconds: ~500 calls
- Relay toggles: Depends on PWM duty cycle, but should be a pattern

## If You're Still Stuck

1. **Capture full boot log:** Save all serial output from startup
2. **Run for 2 minutes:** Let it stabilize, collect 5-second status logs
3. **Check temperature trends:** Should gradually increase if heating
4. **Look for ERROR messages:** Any ERROR or FATAL messages are critical clues

### Debug Output File Collection
```bash
# Capture first 30 seconds of logs to a file
timeout 30 screen -S debug -L -Logfile esp32_debug.log /dev/ttyUSB0 115200

# Then analyze:
cat esp32_debug.log | grep "ERROR\|FATAL\|WARNING"
cat esp32_debug.log | grep "ISR\|LOOP STATUS"
cat esp32_debug.log | grep "State transition"
```

## Key Files Containing Debug Code

- **ISR counters:** `include/clevercoffee/isr.h` lines 17-20
- **Main loop logging:** `src/main.cpp` lines 182-217
- **State machine logging:** `src/core/LoopManager.cpp` lines 548-590
- **Process control logging:** `src/core/LoopManager.cpp` lines 235-260
- **SystemInitializer logging:** `src/core/SystemInitializer.cpp` lines 126-140

## Next Steps After Hardware Test

Once you've identified which part is not working:

### ISR Not Firing
- Check if context is nullptr after setting
- Verify timer pointer is valid
- Check if enableTimer1() is even being called

### Relay Not Toggling
- Verify PID output is changing (>0 when heating)
- Check if relay object exists
- Test relay directly if possible

### Temperature Not Reading
- Check I2C or Dallas wire connections
- Verify sensor type in Config.h
- Look for sensor communication errors in logs

### State Machine Stuck
- Check condition() function in current state
- Verify all handlers are initialized
- Look for state transition exceptions

---

**Build Info:**
- Commit with debug logging: Check git log
- Debug logging level: INFO by default, DEBUG via telnet or web UI
- ISR monitoring: No serial output needed, uses volatile counters
