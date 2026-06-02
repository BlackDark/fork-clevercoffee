# Repository Summary

## Project Overview

**CleverCoffee** is an ESP32-based coffee machine controller (forked from rancilio-pid/clevercoffee). It provides PID temperature control, state machine-based brewing workflows, web-based UI, and OTA updates for Rancilio Silvia coffee machines.

### Key Features
- PID temperature control for brew and steam modes
- State machine-based process control
- React-based web UI (precompiled, served from LittleFS)
- WiFi configuration with hostname support
- OTA firmware updates
- MQTT integration
- Configuration persistence via NVS (non-volatile storage)
- Modern C++20/23 codebase

## Repository Structure

```
├── src/                    # Implementation files (.cpp)
├── include/clevercoffee/   # Header files (.h) organized by domain
│   ├── context/           # System context and shared state
│   ├── control/           # Process controllers, emergency stop
│   ├── coordinators/      # Component coordinators (sensor, UI, network)
│   ├── core/              # System initialization
│   ├── display/           # OLED display management
│   ├── handlers/          # Brew, steam, hot water, power handlers
│   ├── hardware/          # Hardware abstractions (relays, switches, sensors)
│   ├── network/           # WiFi, MQTT, web server
│   ├── state/             # State machine implementation
│   └── utils/             # Utilities (timers, resilience, helpers)
├── test/                  # Unit tests (Google Test)
│   ├── mocks/              # Mock implementations
│   └── test_*/         # Test suites organized by component
├── frontend/              # Static web assets (HTML, CSS, JS)
└── ui/                    # React frontend source (TypeScript/TSX)
```

## Build System

**Platform:** PlatformIO with ESP32 (Arduino framework)  
**C++ Standard:** C++20/23 (`-std=gnu++2a`)  
**Target Board:** AZ-Delivery DevKit V4 (ESP32)

### Build Commands

```bash
# Build for ESP32 (silent)
~/.platformio/penv/bin/pio run -e esp32_usb -s

# Build with verbose output
~/.platformio/penv/bin/pio run -e esp32_usb

# Format code
~/.platformio/penv/bin/pio run --target format -e esp32_usb -s

# Check formatting (CI)
~/.platformio/penv/bin/pio run --target check-format -e esp32_usb -s
```

### Build Environments

- `esp32_usb`: Main development environment (USB upload, debugging)
- `esp32_ota`: OTA update environment
- `native_test`: Native unit tests (Google Test)

### Pre-build Scripts

- CI sets `CLEVERCOFFEE_VERSION` and `PLATFORMIO_BUILD_FLAGS=-D VERSION=\"…\"`; local builds use `dev` from `defaults.h` unless you export both env vars
- `run_clangformat.py`: Formats code before build
- `build_frontend.py`: Compiles React frontend to static assets

## Testing

### Test-Driven Development (TDD)

**Framework:** Google Test (GTest) + Google Mock (GMock)  
**Test Environment:** Native tests (`native_test` env) + ESP32 tests (`esp32_usb` env)

### Running Tests

```bash
# Run all tests
pio test

# Run specific test suite
pio test -f test_emergency_stop_manager

# Verbose output
pio test --verbose
```

### Test Structure

- Tests in `test/test_*/test_main.cpp` files
- Mocks in `test/mocks/`
- Test helpers in `test/test_utils/TestHelpers.h`
- Base fixture: `TestFixtureBase` (provides SystemContext)

### Testing Patterns

1. **Dependency Injection**: Prefer constructor injection over singletons
   ```cpp
   // Good: Testable
   SensorCoordinator coord(&mockSensor, nullptr, nullptr);
   
   // Avoid: Hard to test
   Config::getInstance().pidEnabled.get();
   ```

2. **Mock External Dependencies**: Use mocks for hardware, network, storage
   - `MockISensor`, `MockRelay`, `MockSwitch`, `MockLED`
   - `MockConfig` (where dependency injection is available)

3. **Test Coverage Goals**:
   - Core components: 80%+ (SystemContext, ProcessController, State Machine)
   - Handlers: 70%+ (Brew, Steam, Power, HotWater)
   - Hardware: 60%+ (HardwareManager, sensors)

### Known Testing Limitations

- `Config::getInstance()` singleton used extensively (test with real instance, reset between tests)
- Some hardware requires actual initialization (use mocks or integration tests)
- ISR code is hard to test (test logic separately)

## Coding Standards

### Modern C++ Practices

- **C++ Standard**: C++20/23 (ESP32 limitations: no concepts, expected, format, consteval)
- **Memory**: RAII, smart pointers (`unique_ptr`, `shared_ptr`), Rule of Zero/Three/Five
- **Style**: `#pragma once`, const correctness, `constexpr` where applicable
- **STL**: Prefer algorithms over raw loops
- **No backward compatibility**: Clean, modern code only

### Code Style (clang-format)

- **Indentation**: 4 spaces
- **Braces**: Opening brace on same line
- **Formatting**: Enforced by CI, run `pio run --target format` before commit
- **Pre-commit**: Use `pre-commit install` for automatic formatting

### Architecture Patterns

1. **State Machine**: Process control via state machine (`StateMachine`, `MachineState`)
2. **Coordinators**: Component coordination (SensorCoordinator, UICoordinator, NetworkCoordinator)
3. **Context Pattern**: `SystemContext` centralizes shared state
4. **Dependency Injection**: Coordinators and handlers accept dependencies via constructor
5. **Hardware Abstraction**: Interfaces for hardware (ISensor, Relay, Switch, LED)

## Development Workflow

1. **Before Editing**: Verify project builds: `~/.platformio/penv/bin/pio run -e esp32_usb -s`
2. **Write Tests First**: Follow TDD - write failing test, implement, refactor
3. **Use Mocks**: Mock hardware/network dependencies in tests
4. **Format Code**: Run `pio run --target format` before committing
5. **Verify Build**: Ensure code compiles after changes

## Key Dependencies

- **ArduinoJson** (7.4.2): Configuration and JSON handling
- **U8g2lib**: OLED display
- **ESPAsyncWebServer**: Web server
- **WiFiManager**: WiFi configuration
- **PID Library**: Temperature control
- **DallasTemperature/HX711**: Sensors
- **NimBLE**: Bluetooth (for scales)

## Documentation

- `CLAUDE.md`: Agent instructions (C++ standards, project structure)
- `TESTING_GUIDE.md`: Comprehensive testing guide
- `CONTRIBUTING.md`: Code style and contribution guidelines
- `CONFIG_REFERENCE.md`: Configuration reference

## Important Notes

- **ESP32 Limitations**: No C++23 concepts, `std::expected`, `std::format`, `std::is_constant_evaluated()`
- **Always test builds** before starting edits
- **Avoid new dependencies** unless absolutely necessary
- **Prefer compile-time errors** over runtime errors
- **Follow C++ Core Guidelines**
