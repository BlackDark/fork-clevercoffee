# CleverCoffee ESP32 Testing Documentation Index

## Quick Links

- **For Quick Start:** Read [TESTING_QUICK_START.md](TESTING_QUICK_START.md) (10 minutes)
- **For Deep Dive:** Read [ESP32_TESTING_CAPABILITIES.md](ESP32_TESTING_CAPABILITIES.md) (30 minutes)
- **For Implementation:** Follow [TESTING_QUICK_START.md](TESTING_QUICK_START.md) sections

---

## What's Covered

### TESTING_QUICK_START.md
**Purpose:** Get up and running with tests in 30 minutes

**Sections:**
1. TL;DR (2 minutes)
2. Setup (30 minutes)
3. Desktop Testing Example (GoogleTest)
4. Device Testing Example (Unity)
5. Common Test Patterns (reusable templates)
6. Running Tests (commands)
7. Integration with Bug Fixes
8. Troubleshooting

**Best For:** Developers ready to write tests now

---

### ESP32_TESTING_CAPABILITIES.md
**Purpose:** Comprehensive research on ESP32 testing options

**Sections:**
1. GoogleTest + GoogleMock on ESP32 (Answer: PARTIAL - desktop only)
2. Other C++ Mocking Libraries (7 options reviewed)
3. Practical Limitations of Testing on ESP32 (memory, real-time)
4. Testing Hardware-Dependent Code (patterns)
5. Recommended Testing Strategy (three-tier approach)
6. Mocking Hardware Layers (abstraction strategy)
7. Tools & Libraries for ESP32 (registry search results)
8. Example: Testing Emergency Temperature (complete test suite)
9. Summary Table (what's possible)
10. Action Items (4-phase plan)
11. Files to Reference

**Best For:** Understanding the full picture and making architectural decisions

---

## Questions Answered

### 1. Can We Use GoogleTest + GoogleMock on ESP32?
**Answer:** PARTIAL - Only on Desktop/Native

See: [ESP32_TESTING_CAPABILITIES.md § 1](ESP32_TESTING_CAPABILITIES.md#1-can-we-use-googletest--googlemock-on-esp32)

Key Points:
- GoogleTest v1.15.2 available in PlatformIO
- Works great on Linux/macOS
- Cannot run on ESP32 (needs ~100KB RAM, ESP32 has ~200KB total)
- Solution: Use GoogleTest on desktop, Unity on device

---

### 2. Can We Use Other C++ Mocking Libraries on ESP32?
**Answer:** YES - Multiple Options Available

See: [ESP32_TESTING_CAPABILITIES.md § 2](ESP32_TESTING_CAPABILITIES.md#2-other-c-mocking-libraries-on-esp32)

Available:
- **Desktop:** GoogleTest, Catch2, FakeIt, CMock
- **Device:** Unity (built-in), ArduinoFake, Mock by berrak

Recommendation: **Unity (device) + GoogleTest (desktop)**

---

### 3. What Are the Practical Limitations of Testing on ESP32?
**Answer:** Memory and Real-Time Constraints

See: [ESP32_TESTING_CAPABILITIES.md § 3](ESP32_TESTING_CAPABILITIES.md#3-practical-limitations-of-testing-on-esp32)

Constraints:
- ESP32 SRAM: ~320KB total (~50-80KB available for tests)
- Limited to 20-30 tests per batch
- Cannot test ISR timing, WiFi timing, precise PWM
- Solution: Test logic on desktop, verify timing on device

---

### 4. How Do Projects Typically Test Hardware-Dependent Code?
**Answer:** Abstraction Layers + Mocking

See: [ESP32_TESTING_CAPABILITIES.md § 4](ESP32_TESTING_CAPABILITIES.md#4-testing-hardware-dependent-code-patterns)

Your Code Already Has:
- Abstract TempSensor interface ✅
- HardwareManager factory ✅
- Can swap mock implementations ✅

Patterns:
- Desktop: Mock sensors/relays + real business logic
- Device: Real hardware + integration tests

---

### 5. What's the Best Approach for This Codebase?
**Answer:** Three-Tier Testing Strategy

See: [ESP32_TESTING_CAPABILITIES.md § 5](ESP32_TESTING_CAPABILITIES.md#5-recommended-testing-strategy-for-clevercoffee)

Three Tiers:
1. **Unit Tests (Desktop):** GoogleTest, 100+ tests, < 1 second
2. **Integration Tests (Desktop):** GoogleTest, mock only hardware
3. **Device Tests (ESP32):** Unity, real hardware, 20-30 tests

---

### 6. Can We Mock Hardware Layers Effectively?
**Answer:** YES - Your Code Supports It

See: [ESP32_TESTING_CAPABILITIES.md § 6](ESP32_TESTING_CAPABILITIES.md#6-mocking-hardware-layers-effectively)

Example:
```cpp
class MockTempSensor : public TempSensor {
    bool sample_temperature(double& temperature) const override {
        temperature = 95.0;  // Return fixed value
        return true;
    }
};
```

---

### 7. What Tools/Libraries Are Recommended?
**Answer:** Unity + GoogleTest Combination

See: [ESP32_TESTING_CAPABILITIES.md § 7](ESP32_TESTING_CAPABILITIES.md#7-tools--libraries-for-esp32-testing)

**For Desktop:**
- google/googletest v1.15.2 ✅
- doctest v2.4.11 (alternative)
- Catch2 v4.3.7 (alternative)

**For Device:**
- Unity v2.6.0 (built-in to ESP32 SDK) ✅
- ArduinoFake v0.4.0 (optional, for Arduino APIs)

---

### 8. How Do We Test the Emergency Temperature Fix?
**Answer:** Complete Example Provided

See: [ESP32_TESTING_CAPABILITIES.md § 8](ESP32_TESTING_CAPABILITIES.md#8-example-testing-emergency-temperature-detection)

Desktop Test Suite (GoogleTest):
- TriggersAboveThreshold
- AllowsNormalTemperature
- DebouncesHighTemperature
- HysteresisPreventsFalseRecovery
- DetectsSensorDisconnection
- DetectsShortCircuit
- RespectConfigurableThreshold
- ResetsDebounceOnStateChange

Device Test (Unity):
- Real sensor integration
- Memory stability

---

## Implementation Timeline

### Phase 1: Setup (1 hour)
- Add test environments to platformio.ini
- Create test directory structure
- Verify tools installation

**File:** [TESTING_QUICK_START.md § Setup](TESTING_QUICK_START.md#setup-30-minutes)

### Phase 2: Mock Layer (2 hours)
- MockTempSensor.h
- MockRelay.h
- MockSwitch.h
- MockHardwareManager.h

**File:** [TESTING_QUICK_START.md § Create First Mock](TESTING_QUICK_START.md#step-3-create-first-mock)

### Phase 3: Unit Tests (3 hours)
- Emergency temperature tests
- State machine tests
- ISR race condition tests
- Null pointer check tests

**File:** [ESP32_TESTING_CAPABILITIES.md § Example](ESP32_TESTING_CAPABILITIES.md#8-example-testing-emergency-temperature-detection)

### Phase 4: Device Tests (2 hours)
- Real sensor integration
- Memory leak detection
- ISR timing verification
- Relay control verification

**File:** [TESTING_QUICK_START.md § Device Testing](TESTING_QUICK_START.md#device-testing-example-unity)

---

## Key Statistics

### What We Researched
- ✅ 7 mocking frameworks evaluated
- ✅ Memory constraints analyzed
- ✅ Real-time limitations documented
- ✅ 3-tier testing architecture designed
- ✅ 8 test examples provided
- ✅ 4-phase implementation plan created

### Documentation Provided
- ✅ 671 lines of comprehensive research (ESP32_TESTING_CAPABILITIES.md)
- ✅ 364 lines of quick start guide (TESTING_QUICK_START.md)
- ✅ Complete code examples
- ✅ Troubleshooting guide
- ✅ Implementation roadmap

### Tools Available
- ✅ GoogleTest v1.15.2 (PlatformIO registry)
- ✅ Unity v2.6.0 (built into ESP32 SDK)
- ✅ Catch2 v4.3.7 (PlatformIO registry)
- ✅ ArduinoFake v0.4.0 (PlatformIO registry)

---

## Integration with Bug Fixes

All 8 critical bugs can be tested:

| Bug | Test Type | Location |
|-----|-----------|----------|
| MEM-001: TempSensorTSIC leak | Memory | Device test (extended) |
| MEM-002: TempSensorDallas leak | Memory | Device test (extended) |
| ISR-001: Race condition | Integration | Device test + oscilloscope |
| STATE-001: Null returns | Unit | GoogleTest desktop |
| STATE-002: Static variable | Unit | GoogleTest desktop |
| HW-001: Const violation | Compile | clang-format (existing) |
| CTRL-001: Null check | Unit | GoogleTest desktop |
| CTRL-002: Emergency temp | Unit + Device | Both (complete example) |

---

## Next Steps

### For Quick Implementation
1. Open [TESTING_QUICK_START.md](TESTING_QUICK_START.md)
2. Follow "Setup (30 minutes)" section
3. Copy "Create First Mock" code
4. Write first test
5. Run: `pio test -e native_test`

### For Deep Understanding
1. Read [ESP32_TESTING_CAPABILITIES.md](ESP32_TESTING_CAPABILITIES.md)
2. Review sections 1-7 for context
3. Study section 8 (emergency temperature example)
4. Review action items (section 10)
5. Then proceed with "Quick Implementation"

### For Architecture Review
1. Read section 4 & 5 of [ESP32_TESTING_CAPABILITIES.md](ESP32_TESTING_CAPABILITIES.md)
2. Review three-tier architecture diagram
3. Verify hardware mocking strategy aligns with your code
4. Approve implementation plan
5. Proceed with phases

---

## Testing Checklist

### Setup Phase
- [ ] Read TESTING_QUICK_START.md setup section
- [ ] Add [env:native_test] to platformio.ini
- [ ] Add [env:esp32_test] to platformio.ini
- [ ] Create test/mocks/ directory
- [ ] Create test/desktop/ directory
- [ ] Create test/device/ directory
- [ ] Verify pio test --list-tests works

### Mock Layer Phase
- [ ] Implement MockTempSensor.h
- [ ] Implement MockRelay.h
- [ ] Implement MockSwitch.h
- [ ] Test mocks compile
- [ ] Verify mock behavior with simple test

### Unit Tests Phase
- [ ] Write emergency temperature tests (8 tests)
- [ ] Write state machine tests (5-10 tests)
- [ ] Write ISR race condition tests (3-4 tests)
- [ ] Write null pointer tests (2-3 tests)
- [ ] Verify desktop tests pass
- [ ] Achieve 80%+ code coverage

### Device Tests Phase
- [ ] Write real sensor integration test
- [ ] Write memory stability test (30 seconds)
- [ ] Write ISR timing verification test
- [ ] Write relay control test
- [ ] Verify device tests pass on hardware
- [ ] Monitor for memory leaks
- [ ] Check logs for errors

---

## Resources

### Documentation
- [TESTING_QUICK_START.md](TESTING_QUICK_START.md) - Start here (10 min)
- [ESP32_TESTING_CAPABILITIES.md](ESP32_TESTING_CAPABILITIES.md) - Complete research (30 min)

### External Resources
- GoogleTest Documentation: https://google.github.io/googletest/
- Unity Test Framework: https://github.com/ThrowTheSwitch/Unity
- PlatformIO Testing: https://docs.platformio.org/en/latest/advanced/unit-testing/
- ESP32 SDK Unity: ~/.platformio/packages/framework-arduinoespressif32/tools/sdk/esp32/include/unity/

### Your Code
- Abstract Interfaces: include/clevercoffee/hardware/tempsensors/TempSensor.h
- Hardware Manager: include/clevercoffee/hardware/HardwareManager.h
- Process Controller: src/control/ProcessController.cpp
- Emergency Logic: docs/plans/2025-12-26-critical-bug-fixes.md (Task 8)

---

## FAQ

**Q: Should we test on device or desktop?**  
A: Both! Desktop for fast feedback, device for hardware verification.

**Q: How long does setup take?**  
A: 1 hour to configure, 2 hours for mock layer, then write tests.

**Q: Do we need GoogleTest on the device?**  
A: No! Use Unity (built-in) on device, GoogleTest on desktop.

**Q: Can we test WiFi/BLE?**  
A: Partially on device (stub on desktop). WiFi timing is not deterministic.

**Q: How many tests can we run on ESP32?**  
A: 20-30 per batch due to memory. Desktop has no limit.

**Q: Does the project support mocking?**  
A: Yes! Already has abstract interfaces and HardwareManager.

**Q: Which is better: Catch2 or GoogleTest?**  
A: For this project, GoogleTest (more tests, better mocking).

**Q: Can we use for emergency temperature fix?**  
A: Yes! Section 8 provides complete test suite.

---

## Summary

This documentation package provides:
- ✅ Complete answers to all 7 questions
- ✅ Implementation guide with code examples
- ✅ Testing strategy aligned with your architecture
- ✅ Support for all 8 critical bug fixes
- ✅ Tools already available in PlatformIO
- ✅ 4-phase implementation roadmap
- ✅ Troubleshooting and patterns

**Ready to start testing!**

---

*Documentation compiled: December 26, 2025*  
*Project: CleverCoffee ESP32 Arduino*  
*Status: Complete and Ready to Implement*
