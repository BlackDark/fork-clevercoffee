# Parameter System Test Suite

This directory contains comprehensive tests for the CleverCoffee parameter system, ensuring that all parameter operations work correctly without requiring ESP32 hardware.

## Test Files

### Core Tests

- **`test_comprehensive_parameter_system.cpp`** - Complete parameter system test suite
  - Basic parameter operations (set/get)
  - Parameter validation (ranges, types)
  - NVS persistence (save/load)
  - Enum options structure
  - Global variable binding
  - Error handling

- **`test_enum_static_mappings.cpp`** - Enum static mapping tests
  - Static enum option mappings
  - Ordering stability tests
  - Value lookup functions

- **`test_web_api_compatibility.cpp`** - Web API compatibility tests
  - JSON serialization
  - Type mapping verification
  - Frontend compatibility

- **`test_enum_types.cpp`** - Basic enum type value tests
  - Enum value verification

### Test Scripts

- **`../run_tests.sh`** - Simple test runner for individual tests
- **`../test_all.sh`** - Comprehensive test suite runner with colored output

## Running Tests

### Quick Test (Individual)
```bash
# From project root
./run_tests.sh
```

### Comprehensive Test Suite
```bash
# From project root
./test_all.sh
```

### Manual Compilation
```bash
# Compile individual test
g++ -std=c++17 -O2 -Wall -Wextra test/test_comprehensive_parameter_system.cpp -o test/parameter_system_test

# Run test
./test/parameter_system_test
```

## Test Coverage

### ✅ Verified Functionality

1. **Parameter Operations**
   - Setting and getting all parameter types (bool, int, uint8, double, float, string, enum)
   - Type safety and validation
   - Range checking for numeric types
   - String length validation

2. **NVS Persistence**
   - Saving parameters to mock NVS storage
   - Loading parameters from NVS storage
   - Reset to default values
   - Persistence across simulated restarts

3. **Enum System**
   - Static enum option mappings with explicit values
   - Ordering stability (reordering doesn't break saved values)
   - Value/label lookup functions
   - Frontend-compatible JSON structure

4. **Global Variable Binding**
   - Direct binding between parameters and global variables
   - Immediate updates when parameters change
   - Consistent state across parameter and global variable access

5. **Web API Compatibility**
   - Correct type value mappings (ENUM=5, BOOL=6, etc.)
   - Proper JSON structure for enum options
   - Frontend-compatible parameter serialization

6. **Error Handling**
   - Non-existent parameter rejection
   - Type mismatch prevention
   - Out-of-range value rejection

### 🔧 Mock Components

The tests use mock implementations of ESP32-specific components:

- **MockPreferences** - Simulates ESP32 NVS storage
- **String** - Mock Arduino String class
- **MockJson** - Simplified JSON structure for testing

### 🎯 Benefits

1. **No Hardware Required** - Tests run on any development machine
2. **Fast Execution** - Complete test suite runs in seconds
3. **Comprehensive Coverage** - Tests all major parameter system functionality
4. **Regression Prevention** - Catches breaking changes early
5. **Documentation** - Tests serve as usage examples

## Integration with ESP32 Build

The parameter system is also tested through actual ESP32 compilation:

```bash
# Test ESP32 compilation
~/.platformio/penv/bin/platformio run --environment esp32_usb
```

This ensures that:
- All code compiles correctly for ESP32
- No syntax or linking errors
- Real hardware compatibility

## Test Results

When all tests pass, you'll see:

```
🎉 ALL TESTS PASSED! (4/4)
✅ Parameter system is working correctly

Test Coverage:
• ✅ Basic parameter operations (set/get)
• ✅ Parameter validation (ranges, types)
• ✅ NVS persistence (save/load)
• ✅ Enum options with static mappings
• ✅ Global variable binding
• ✅ Error handling
• ✅ Enum ordering stability
• ✅ Web API JSON serialization
• ✅ Frontend compatibility

The parameter system is ready for production! 🚀
```

## Adding New Tests

To add new tests:

1. Create a new `.cpp` file in the `test/` directory
2. Include necessary mock headers
3. Write test functions following the pattern:
   ```cpp
   void testNewFeature() {
       std::cout << "\n=== Testing New Feature ===" << std::endl;
       // Test implementation
       assert(condition);
       std::cout << "✓ Test description" << std::endl;
   }
   ```
4. Add the test to `test_all.sh`

## Troubleshooting

### Compilation Errors
- Ensure you have a C++17 compatible compiler
- Check that all test files are in the correct location
- Verify `#include` paths are correct

### Test Failures
- Check the assertion that failed
- Verify mock data setup
- Ensure test isolation (tests don't affect each other)

### ESP32 Compilation Issues
- Verify PlatformIO installation
- Check that all libraries are available
- Ensure configuration files are correct

---

This test suite provides confidence that the parameter system works correctly across all use cases, from basic operations to complex enum mappings and web API integration.
