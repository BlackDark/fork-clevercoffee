# Timer Migration Guide

## Overview
This guide explains how to migrate from the legacy `Timer` class to the modern `ModernTimer` class for better performance and type safety.

## Key Benefits of ModernTimer

1. **Type Safety**: Uses `std::chrono` types instead of raw milliseconds
2. **Precision**: Better timing accuracy with steady_clock
3. **No Drift**: Automatic drift correction for long-running timers
4. **More Features**: Additional methods for timing inspection
5. **C++20 Concepts**: Type-safe template constraints

## Migration Examples

### Basic Timer Creation

**Old (Legacy Timer):**
```cpp
#include "Timer.h"

// Create 1-second timer
auto timer = Timer([]() { 
    LOG(INFO, "Timer fired!"); 
}, 1000);
```

**New (ModernTimer):**
```cpp
#include "ModernTimer.h"

// Create 1-second timer with type safety
auto timer = ModernTimer([]() { 
    LOG(INFO, "Timer fired!"); 
}, std::chrono::seconds(1));

// Or use convenience factory
auto timer2 = make_timer([]() { 
    LOG(INFO, "Timer fired!"); 
}, std::chrono::milliseconds(1000));

// For legacy compatibility
auto timer3 = make_legacy_timer([]() { 
    LOG(INFO, "Timer fired!"); 
}, 1000); // milliseconds
```

### Timer Operations

**Old:**
```cpp
timer.pause();
timer.resume();
timer.reset();
// Limited inspection capabilities
```

**New:**
```cpp
timer.pause();
timer.resume();
timer.reset();

// Enhanced inspection
auto remaining = timer.getTimeRemaining();
auto since_last = timer.getTimeSinceLastExecution();
bool overdue = timer.isOverdue();
bool running = timer.isRunning();

// Dynamic interval changes
timer.setInterval(std::chrono::milliseconds(500));
```

### Different Duration Types

```cpp
// Microsecond precision for high-frequency tasks
auto fast_timer = ModernTimer<std::chrono::microseconds>(
    []() { /* high freq task */ }, 
    std::chrono::microseconds(100)
);

// Second precision for slow tasks
auto slow_timer = ModernTimer<std::chrono::seconds>(
    []() { /* periodic cleanup */ }, 
    std::chrono::seconds(30)
);

// Convenient aliases
MillisecondTimer ms_timer(callback, std::chrono::milliseconds(100));
SecondTimer sec_timer(callback, std::chrono::seconds(5));
MicrosecondTimer us_timer(callback, std::chrono::microseconds(50));
```

## Gradual Migration Strategy

### Phase 1: Add ModernTimer alongside existing Timer
- Keep existing Timer class for compatibility
- Add ModernTimer.h to the project
- Start using ModernTimer for new code

### Phase 2: Migrate high-frequency timers first
- Convert timing-critical code (PID controllers, sensor reading)
- Use microsecond or nanosecond precision where needed

### Phase 3: Migrate remaining timers
- Convert UI update timers, periodic cleanup tasks
- Use appropriate duration types (seconds for slow tasks)

### Phase 4: Deprecate old Timer (optional)
- Mark Timer class as deprecated
- Provide legacy compatibility layer if needed

## Performance Comparison

| Feature | Legacy Timer | ModernTimer |
|---------|-------------|-------------|
| Resolution | ~1ms (millis()) | ~1ns (steady_clock) |
| Drift | Accumulates over time | Automatic correction |
| Type Safety | Raw unsigned long | std::chrono types |
| Inspection | Limited | Full timing info |
| Memory | ~32 bytes | ~40 bytes |
| Performance | Arduino millis() | std::chrono (faster) |

## Example: Migrating a PID Controller Timer

**Before:**
```cpp
class PIDController {
    Timer updateTimer_;
    
public:
    PIDController() : updateTimer_(
        [this]() { this->update(); }, 
        50  // 50ms = 20Hz update rate
    ) {}
};
```

**After:**
```cpp
class PIDController {
    MicrosecondTimer updateTimer_;
    
public:
    PIDController() : updateTimer_(
        [this]() { this->update(); }, 
        std::chrono::microseconds(50000)  // 50ms with μs precision
    ) {}
    
    // New capabilities
    double getUpdateJitter() {
        auto expected = updateTimer_.getInterval();
        auto actual = updateTimer_.getTimeSinceLastExecution();
        return std::chrono::duration<double, std::milli>(actual - expected).count();
    }
};
```

## Best Practices

1. **Choose appropriate duration types:**
   - Use `std::chrono::microseconds` for high-frequency control loops
   - Use `std::chrono::milliseconds` for UI updates
   - Use `std::chrono::seconds` for periodic tasks

2. **Use factory functions for clarity:**
   ```cpp
   auto timer = make_timer(callback, 100ms);  // C++14 literal
   ```

3. **Monitor timing performance:**
   ```cpp
   if (timer.isOverdue()) {
       LOG(WARNING, "Timer is behind schedule!");
   }
   ```

4. **Prefer steady_clock for timing:**
   - ModernTimer uses `steady_clock` which is monotonic
   - Won't be affected by system clock adjustments
   - Better for real-time applications

This migration can be done incrementally without breaking existing functionality.