/**
 * @file constexpr_demo.cpp
 * @brief Demonstration of C++23 constexpr validation in CleverCoffee
 * 
 * This file shows how compile-time validation catches configuration errors
 * before the code ever runs on the ESP32.
 */

#include "../src/constexpr_validation.h"
#include "../src/Config.h"

#if __cplusplus >= 202002L

using namespace CleverCoffee::Validation;

// ============================================================================
// COMPILE-TIME VALIDATION EXAMPLES
// ============================================================================

namespace ConstexprDemo {

// Valid configurations - these compile successfully
constexpr bool demonstrateValidConfigs() {
    // Valid temperature settings
    constexpr auto valid_brew_temp = ValidatedBrewTemp{95.0};    // ✅ OK
    constexpr auto valid_steam_temp = ValidatedSteamTemp{125.0}; // ✅ OK
    constexpr auto valid_pid_kp = ValidatedPidKp{60.0};          // ✅ OK
    
    return isValidBrewTemperature(95.0) &&     // ✅ True at compile-time
           isValidSteamTemperature(125.0) &&   // ✅ True at compile-time  
           isValidPidKp(60.0);                 // ✅ True at compile-time
}

// These would cause COMPILE ERRORS if uncommented:
/*
constexpr auto invalid_brew_temp = ValidatedBrewTemp{300.0};    // ❌ Compile error!
constexpr auto invalid_steam_temp = ValidatedSteamTemp{50.0};   // ❌ Compile error!
constexpr auto invalid_pid_kp = ValidatedPidKp{-10.0};         // ❌ Compile error!
*/

// Force compile-time validation
static_assert(demonstrateValidConfigs(), "Configuration validation failed");

// ============================================================================
// RUNTIME VALIDATION WITH COMPILE-TIME OPTIMIZATION
// ============================================================================

// This function gets optimized to a constant when called with constexpr values
bool validateUserInput(double brewTemp, double steamTemp, double pidKp) {
    // These calls get optimized to compile-time constants when possible
    return isValidBrewTemperature(brewTemp) && 
           isValidSteamTemperature(steamTemp) && 
           isValidPidKp(pidKp);
}

// Example usage in CleverCoffee code
void demonstrateRuntimeValidation() {
    // User inputs from web interface
    double userBrewTemp = 92.5;
    double userSteamTemp = 130.0;
    double userPidKp = 75.0;
    
    if (validateUserInput(userBrewTemp, userSteamTemp, userPidKp)) {
        // Safe to apply these settings
        MODERN_LOG(INFO, "Configuration validated: brew={:.1f}°C, steam={:.1f}°C, Kp={:.1f}", 
                  userBrewTemp, userSteamTemp, userPidKp);
    } else {
        // Invalid settings detected
        MODERN_LOG(ERROR, "Invalid configuration detected: brew={:.1f}°C, steam={:.1f}°C, Kp={:.1f}", 
                  userBrewTemp, userSteamTemp, userPidKp);
    }
}

// ============================================================================
// ENHANCED CONFIG PARAMETER EXAMPLES  
// ============================================================================

void demonstrateValidatedParameters() {
    // Access the new validated parameters from Config
    auto& config = Config::getInstance();
    
#if __cplusplus >= 202002L
    // C++23 validated parameters provide additional safety
    double brewTemp = config.brewSetpointValidated.get();
    double steamTemp = config.steamSetpointValidated.get(); 
    double pidKp = config.pidRegularKpValidated.get();
    
    MODERN_LOG(INFO, "Validated config: brew={:.1f}°C, steam={:.1f}°C, Kp={:.1f}", 
              brewTemp, steamTemp, pidKp);
    
    // Attempting to set invalid values will fail gracefully
    if (!config.brewSetpointValidated.set(300.0)) {  // Invalid temperature
        MODERN_LOG(WARNING, "Rejected invalid brew temperature: 300.0°C");
    }
    
    if (!config.pidRegularKpValidated.set(-10.0)) {  // Invalid PID parameter
        MODERN_LOG(WARNING, "Rejected invalid PID Kp: -10.0");
    }
#endif
}

// ============================================================================
// PERFORMANCE COMPARISON
// ============================================================================

// Traditional validation (runtime only)
bool validateTraditional(double temp) {
    return temp >= 20.0 && temp <= 110.0;  // Always executed at runtime
}

// Modern constexpr validation (compile-time when possible)
constexpr bool validateModern(double temp) {
    return isValidBrewTemperature(temp);    // Optimized to constant when temp is constexpr
}

void performanceDemonstration() {
    // Runtime validation - always executes
    bool result1 = validateTraditional(95.0);
    
    // Compile-time validation - optimized away when value is known
    constexpr bool result2 = validateModern(95.0);  // Becomes: constexpr bool result2 = true;
    
    // For dynamic values, both perform similarly at runtime
    double userInput = 92.5;
    bool result3 = validateTraditional(userInput);  // Runtime check
    bool result4 = validateModern(userInput);       // Runtime check (same performance)
    
    MODERN_LOG(DEBUG, "Validation results: {} {} {} {}", result1, result2, result3, result4);
}

// ============================================================================
// INTEGRATION WITH EXISTING CLEVERCOFFEE CODE
// ============================================================================

// Example: Enhanced PID initialization with validation
void initializePIDWithValidation() {
    auto& config = Config::getInstance();
    
    // Get PID parameters with compile-time validation
    double kp = config.pidRegularKp.get();
    double tn = config.pidRegularTn.get();  
    double tv = config.pidRegularTv.get();
    
    // Runtime validation with detailed error messages
    if (!isValidPidKp(kp)) {
        MODERN_LOG(ERROR, "Invalid PID Kp: {:.3f} (range: {:.1f}-{:.1f})", 
                  kp, PID_KP_REGULAR_MIN, PID_KP_REGULAR_MAX);
        kp = AGGKP;  // Use default
    }
    
    if (!isValidPidTn(tn)) {
        MODERN_LOG(ERROR, "Invalid PID Tn: {:.3f} (range: {:.1f}-{:.1f})", 
                  tn, PID_TN_REGULAR_MIN, PID_TN_REGULAR_MAX);
        tn = AGGTN;  // Use default
    }
    
    if (!isValidPidTv(tv)) {
        MODERN_LOG(ERROR, "Invalid PID Tv: {:.3f} (range: {:.1f}-{:.1f})", 
                  tv, PID_TV_REGULAR_MIN, PID_TV_REGULAR_MAX);
        tv = AGGTV;  // Use default
    }
    
    // Calculate derived parameters with validation
    double ki = (tn == 0) ? 0 : kp / tn;
    double kd = tv * kp;
    
    MODERN_LOG(INFO, "PID initialized with validated parameters: Kp={:.3f}, Ki={:.3f}, Kd={:.3f}", 
              kp, ki, kd);
    
    // Apply to actual PID controller
    if (g_state.pid) {
        g_state.pid->SetTunings(kp, ki, kd, 1);
    }
}

} // namespace ConstexprDemo

#endif // __cplusplus >= 202002L

// Usage example for main.cpp integration:
/*
void setup() {
    // ... other initialization ...
    
#if __cplusplus >= 202002L
    ConstexprDemo::demonstrateValidatedParameters();
    ConstexprDemo::initializePIDWithValidation();
#endif
    
    // ... continue with setup ...
}
*/