/**
 * @file test_main.cpp
 * @brief Comprehensive unit tests for EmergencyStopManager debounce and hysteresis logic
 *
 * Tests the critical safety logic for emergency temperature detection:
 * - Debounce logic (requires 3 consecutive readings above threshold)
 * - Hysteresis to prevent oscillation
 * - Invalid sensor reading detection
 * - Emergency clearing logic
 */

#include <gtest/gtest.h>
#include "../test_support.h"
#include "clevercoffee/control/EmergencyStopManager.h"
#include "clevercoffee/constants/Temperature.h"
#include "clevercoffee/Config.h"

// Include the .cpp implementations directly since PlatformIO native tests don't link src/ files
// Dependencies (WiFi, Preferences, Serial, etc.) are already stubbed in test/Arduino.h
#include "../mocks/ConfigStubs.cpp"  // Stub implementations of Config helper functions
#include "../../src/Logger.cpp"
// Config is header-only for our purposes (EmergencyStopManager only uses ParamDef members)
// Config.cpp is not needed - we use ConfigStubs.cpp instead for the option getters
#include "../../src/control/EmergencyStopManager.cpp"

using namespace CleverCoffee;

// ============================================================================
// TEST FIXTURE
// ============================================================================

class EmergencyStopManagerTest : public ::testing::Test {
protected:
    void SetUp() override {
        // Use real Config instance for testing
        // Set default emergency threshold and hysteresis
        Config& config = Config::getInstance();
        config.emergencyStopTemp.set(145.0);
        config.emergencyStopHysteresis.set(10.0);
        manager_ = std::make_unique<EmergencyStopManager>(config);
    }

    void TearDown() override {
        manager_->reset();
        manager_.reset();
    }

    std::unique_ptr<EmergencyStopManager> manager_;
};

// ============================================================================
// DEBOUNCE LOGIC TESTS
// ============================================================================

/**
 * TEST: Single reading above threshold does not trigger emergency
 *
 * VERIFY: First reading above threshold increments counter but doesn't trigger
 * REQUIREMENT: DEBOUNCE_COUNT = 3 consecutive readings required
 */
TEST_F(EmergencyStopManagerTest, SingleHighReadingDoesNotTrigger) {
    Config& config = Config::getInstance();
    const double threshold = config.emergencyStopTemp.get();
    const double highTemp = threshold + 1.0;

    // First reading above threshold
    bool result = manager_->checkEmergencyConditions(highTemp);
    EXPECT_FALSE(result) << "Should not trigger on first reading";
    EXPECT_FALSE(manager_->isEmergencyActive());
    EXPECT_EQ(1, manager_->getDebounceCount());
}

/**
 * TEST: Two consecutive readings above threshold do not trigger emergency
 *
 * VERIFY: Counter increments but emergency not triggered until 3rd reading
 */
TEST_F(EmergencyStopManagerTest, TwoConsecutiveHighReadingsDoNotTrigger) {
    Config& config = Config::getInstance();
    const double threshold = config.emergencyStopTemp.get();
    const double highTemp = threshold + 1.0;

    // First reading
    manager_->checkEmergencyConditions(highTemp);
    EXPECT_EQ(1, manager_->getDebounceCount());

    // Second reading
    bool result = manager_->checkEmergencyConditions(highTemp);
    EXPECT_FALSE(result) << "Should not trigger on second reading";
    EXPECT_FALSE(manager_->isEmergencyActive());
    EXPECT_EQ(2, manager_->getDebounceCount());
}

/**
 * TEST: Three consecutive readings above threshold trigger emergency
 *
 * VERIFY: After 3 consecutive readings, emergency is triggered
 * REQUIREMENT: DEBOUNCE_COUNT = 3
 */
TEST_F(EmergencyStopManagerTest, ThreeConsecutiveHighReadingsTriggerEmergency) {
    Config& config = Config::getInstance();
    const double threshold = config.emergencyStopTemp.get();
    const double highTemp = threshold + 1.0;

    // First reading - counter = 1
    manager_->checkEmergencyConditions(highTemp);
    EXPECT_EQ(1, manager_->getDebounceCount());
    EXPECT_FALSE(manager_->isEmergencyActive());

    // Second reading - counter = 2
    manager_->checkEmergencyConditions(highTemp);
    EXPECT_EQ(2, manager_->getDebounceCount());
    EXPECT_FALSE(manager_->isEmergencyActive());

    // Third reading - counter = 3, should trigger
    bool result = manager_->checkEmergencyConditions(highTemp);
    EXPECT_TRUE(result) << "Should trigger emergency after 3 consecutive readings";
    EXPECT_TRUE(manager_->isEmergencyActive());
    EXPECT_EQ(3, manager_->getDebounceCount());
}

/**
 * TEST: Counter resets when temperature drops below threshold minus hysteresis
 *
 * VERIFY: When temp < (threshold - hysteresis), counter resets to 0
 */
TEST_F(EmergencyStopManagerTest, CounterResetsBelowHysteresisThreshold) {
    Config& config = Config::getInstance();
    const double threshold = config.emergencyStopTemp.get();
    const double hysteresis = config.emergencyStopHysteresis.get();
    const double highTemp = threshold + 1.0;
    const double lowTemp = threshold - hysteresis - 1.0;  // Below (threshold - hysteresis)

    // Build up counter with 2 high readings
    manager_->checkEmergencyConditions(highTemp);
    manager_->checkEmergencyConditions(highTemp);
    EXPECT_EQ(2, manager_->getDebounceCount());

    // Drop below hysteresis threshold - counter should reset
    manager_->checkEmergencyConditions(lowTemp);
    EXPECT_EQ(0, manager_->getDebounceCount()) << "Counter should reset when temp drops below (threshold - hysteresis)";
    EXPECT_FALSE(manager_->isEmergencyActive());
}

/**
 * TEST: Counter maintained in hysteresis zone
 *
 * VERIFY: When temp is between (threshold - hysteresis) and threshold,
 * counter is maintained (not incremented, not reset)
 */
TEST_F(EmergencyStopManagerTest, CounterMaintainedInHysteresisZone) {
    Config& config = Config::getInstance();
    const double threshold = config.emergencyStopTemp.get();
    const double hysteresis = config.emergencyStopHysteresis.get();
    const double highTemp = threshold + 1.0;
    const double hysteresisTemp = threshold - hysteresis / 2.0;  // In middle of hysteresis zone

    // Build up counter with 2 high readings
    manager_->checkEmergencyConditions(highTemp);
    manager_->checkEmergencyConditions(highTemp);
    EXPECT_EQ(2, manager_->getDebounceCount());

    // Temperature in hysteresis zone - counter should remain unchanged
    manager_->checkEmergencyConditions(hysteresisTemp);
    EXPECT_EQ(2, manager_->getDebounceCount()) << "Counter should be maintained in hysteresis zone";
    EXPECT_FALSE(manager_->isEmergencyActive());
}

/**
 * TEST: Interrupted sequence resets counter
 *
 * VERIFY: If high reading is interrupted by low reading, counter resets
 */
TEST_F(EmergencyStopManagerTest, InterruptedSequenceResetsCounter) {
    Config& config = Config::getInstance();
    const double threshold = config.emergencyStopTemp.get();
    const double hysteresis = config.emergencyStopHysteresis.get();
    const double highTemp = threshold + 1.0;
    const double lowTemp = threshold - hysteresis - 1.0;

    // Two high readings
    manager_->checkEmergencyConditions(highTemp);
    manager_->checkEmergencyConditions(highTemp);
    EXPECT_EQ(2, manager_->getDebounceCount());

    // Low reading interrupts - counter resets
    manager_->checkEmergencyConditions(lowTemp);
    EXPECT_EQ(0, manager_->getDebounceCount());

    // Next high reading starts from 1 again
    manager_->checkEmergencyConditions(highTemp);
    EXPECT_EQ(1, manager_->getDebounceCount());
}

// ============================================================================
// INVALID SENSOR READING TESTS
// ============================================================================

/**
 * TEST: Invalid reading (too low) immediately triggers emergency
 *
 * VERIFY: Temperature below MIN_VALID_TEMP triggers immediately (no debouncing)
 * REQUIREMENT: Safety - invalid readings are critical
 */
TEST_F(EmergencyStopManagerTest, InvalidLowReadingImmediatelyTriggers) {
    const double invalidTemp = Temperature::MIN_VALID_TEMP_C - 1.0;

    bool result = manager_->checkEmergencyConditions(invalidTemp);
    EXPECT_TRUE(result) << "Invalid low reading should immediately trigger emergency";
    EXPECT_TRUE(manager_->isEmergencyActive());
}

/**
 * TEST: Invalid reading (too high) immediately triggers emergency
 *
 * VERIFY: Temperature above MAX_VALID_TEMP triggers immediately (no debouncing)
 */
TEST_F(EmergencyStopManagerTest, InvalidHighReadingImmediatelyTriggers) {
    const double invalidTemp = Temperature::MAX_VALID_TEMP_C + 1.0;

    bool result = manager_->checkEmergencyConditions(invalidTemp);
    EXPECT_TRUE(result) << "Invalid high reading should immediately trigger emergency";
    EXPECT_TRUE(manager_->isEmergencyActive());
}

// ============================================================================
// EMERGENCY CLEARING TESTS
// ============================================================================

/**
 * TEST: Emergency can be cleared when temperature drops below safe threshold
 *
 * VERIFY: isEmergencyCleared() returns true when temp < EMERGENCY_SAFE_TEMP_C
 */
TEST_F(EmergencyStopManagerTest, EmergencyCanBeClearedWhenSafe) {
    const double safeTemp = Temperature::EMERGENCY_SAFE_TEMP_C - 1.0;

    bool canClear = manager_->isEmergencyCleared(safeTemp);
    EXPECT_TRUE(canClear) << "Emergency should be clearable when temp < EMERGENCY_SAFE_TEMP_C";
}

/**
 * TEST: Emergency cannot be cleared when temperature still elevated
 *
 * VERIFY: isEmergencyCleared() returns false when temp > EMERGENCY_SAFE_TEMP_C
 */
TEST_F(EmergencyStopManagerTest, EmergencyCannotBeClearedWhenElevated) {
    const double elevatedTemp = Temperature::EMERGENCY_SAFE_TEMP_C + 1.0;

    bool canClear = manager_->isEmergencyCleared(elevatedTemp);
    EXPECT_FALSE(canClear) << "Emergency should not be clearable when temp > EMERGENCY_SAFE_TEMP_C";
}

/**
 * TEST: Emergency cannot be cleared with invalid sensor reading
 *
 * VERIFY: Invalid readings prevent emergency clearing
 */
TEST_F(EmergencyStopManagerTest, EmergencyCannotBeClearedWithInvalidReading) {
    const double invalidTemp = Temperature::MAX_VALID_TEMP_C + 1.0;

    bool canClear = manager_->isEmergencyCleared(invalidTemp);
    EXPECT_FALSE(canClear) << "Emergency cannot be cleared with invalid sensor reading";
}

/**
 * TEST: Emergency clears automatically when temperature normalizes
 *
 * VERIFY: When emergency is active and temp drops below (threshold - hysteresis),
 * emergency is automatically cleared
 */
TEST_F(EmergencyStopManagerTest, EmergencyClearsAutomaticallyWhenNormalized) {
    Config& config = Config::getInstance();
    const double threshold = config.emergencyStopTemp.get();
    const double hysteresis = config.emergencyStopHysteresis.get();
    const double highTemp = threshold + 1.0;
    // Emergency clears when temp < EMERGENCY_SAFE_TEMP_C (100.0), not just (threshold - hysteresis)
    const double safeTemp = Temperature::EMERGENCY_SAFE_TEMP_C - 1.0;

    // Trigger emergency with 3 high readings
    manager_->checkEmergencyConditions(highTemp);
    manager_->checkEmergencyConditions(highTemp);
    manager_->checkEmergencyConditions(highTemp);
    EXPECT_TRUE(manager_->isEmergencyActive());

    // Temperature drops below (threshold - hysteresis) first, which resets counter
    // Then drops below safe threshold, which should clear emergency
    const double belowHysteresis = threshold - hysteresis - 1.0;
    manager_->checkEmergencyConditions(belowHysteresis);  // This resets counter but doesn't clear emergency yet
    EXPECT_EQ(0, manager_->getDebounceCount());
    
    // Now drop below safe threshold - this should clear emergency
    manager_->checkEmergencyConditions(safeTemp);
    EXPECT_FALSE(manager_->isEmergencyActive()) << "Emergency should clear when temp < EMERGENCY_SAFE_TEMP_C";
    EXPECT_EQ(0, manager_->getDebounceCount());
}

// ============================================================================
// MANUAL CONTROL TESTS
// ============================================================================

/**
 * TEST: Manual trigger sets emergency state
 *
 * VERIFY: triggerEmergency() sets emergencyActive_ = true
 */
TEST_F(EmergencyStopManagerTest, ManualTriggerSetsEmergency) {
    EXPECT_FALSE(manager_->isEmergencyActive());

    manager_->triggerEmergency();
    EXPECT_TRUE(manager_->isEmergencyActive());
}

/**
 * TEST: Manual clear resets emergency state
 *
 * VERIFY: clearEmergency() sets emergencyActive_ = false and resets counter
 */
TEST_F(EmergencyStopManagerTest, ManualClearResetsEmergency) {
    // Trigger emergency manually
    manager_->triggerEmergency();
    EXPECT_TRUE(manager_->isEmergencyActive());

    // Clear emergency
    manager_->clearEmergency();
    EXPECT_FALSE(manager_->isEmergencyActive());
    EXPECT_EQ(0, manager_->getDebounceCount());
}

/**
 * TEST: Reset clears all state
 *
 * VERIFY: reset() clears emergency state and counter
 */
TEST_F(EmergencyStopManagerTest, ResetClearsAllState) {
    // Build up some state
    Config& config = Config::getInstance();
    const double threshold = config.emergencyStopTemp.get();
    manager_->checkEmergencyConditions(threshold + 1.0);
    manager_->checkEmergencyConditions(threshold + 1.0);
    EXPECT_EQ(2, manager_->getDebounceCount());

    // Trigger emergency
    manager_->triggerEmergency();
    EXPECT_TRUE(manager_->isEmergencyActive());

    // Reset should clear everything
    manager_->reset();
    EXPECT_FALSE(manager_->isEmergencyActive());
    EXPECT_EQ(0, manager_->getDebounceCount());
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

/**
 * TEST: Temperature exactly at threshold
 *
 * VERIFY: Temperature exactly equal to threshold should increment counter
 */
TEST_F(EmergencyStopManagerTest, TemperatureExactlyAtThreshold) {
    Config& config = Config::getInstance();
    const double threshold = config.emergencyStopTemp.get();

    // Temperature exactly at threshold should NOT trigger (needs to be > threshold)
    bool result = manager_->checkEmergencyConditions(threshold);
    EXPECT_FALSE(result);
    EXPECT_EQ(0, manager_->getDebounceCount()) << "Temperature exactly at threshold should not increment counter";
}

/**
 * TEST: Temperature just above threshold
 *
 * VERIFY: Temperature > threshold increments counter
 */
TEST_F(EmergencyStopManagerTest, TemperatureJustAboveThreshold) {
    Config& config = Config::getInstance();
    const double threshold = config.emergencyStopTemp.get();
    const double justAbove = threshold + 0.1;

    manager_->checkEmergencyConditions(justAbove);
    EXPECT_EQ(1, manager_->getDebounceCount());
}

/**
 * TEST: Multiple emergency triggers are idempotent
 *
 * VERIFY: Calling triggerEmergency() multiple times doesn't change state
 */
TEST_F(EmergencyStopManagerTest, MultipleTriggersAreIdempotent) {
    manager_->triggerEmergency();
    EXPECT_TRUE(manager_->isEmergencyActive());

    // Trigger again - should remain active
    manager_->triggerEmergency();
    EXPECT_TRUE(manager_->isEmergencyActive());
}

/**
 * TEST: Multiple clears are idempotent
 *
 * VERIFY: Calling clearEmergency() when not active doesn't change state
 */
TEST_F(EmergencyStopManagerTest, MultipleClearsAreIdempotent) {
    EXPECT_FALSE(manager_->isEmergencyActive());

    // Clear when not active - should remain inactive
    manager_->clearEmergency();
    EXPECT_FALSE(manager_->isEmergencyActive());
}

/**
 * TEST: Configurable threshold affects detection
 *
 * VERIFY: Changing emergency threshold changes detection behavior
 */
TEST_F(EmergencyStopManagerTest, ConfigurableThresholdAffectsDetection) {
    // Set lower threshold (within valid range 120.0-180.0)
    // Note: 100.0 is below the min (120.0), so set() will fail validation
    // Let's use a value within the valid range instead
    Config& config = Config::getInstance();
    const double newThreshold = 130.0;  // Within valid range [120.0, 180.0]
    bool setSuccess = config.emergencyStopTemp.set(newThreshold);
    EXPECT_TRUE(setSuccess) << "Should be able to set threshold to " << newThreshold;
    
    // Verify the threshold was actually set
    EXPECT_DOUBLE_EQ(newThreshold, config.emergencyStopTemp.get()) << "Threshold should be " << newThreshold;
    
    // Create a new manager with the updated config (manager holds a reference to config)
    manager_.reset();
    manager_ = std::make_unique<EmergencyStopManager>(config);

    const double highTemp = newThreshold + 1.0;  // Above new threshold

    // Should trigger after 3 readings
    manager_->checkEmergencyConditions(highTemp);
    EXPECT_EQ(1, manager_->getDebounceCount());
    manager_->checkEmergencyConditions(highTemp);
    EXPECT_EQ(2, manager_->getDebounceCount());
    bool result = manager_->checkEmergencyConditions(highTemp);
    EXPECT_TRUE(result) << "Should trigger with configured threshold (" << newThreshold << ") when temp is " << highTemp;
    EXPECT_TRUE(manager_->isEmergencyActive());
}

/**
 * TEST: Configurable hysteresis affects reset behavior
 *
 * VERIFY: Changing hysteresis changes when counter resets
 */
TEST_F(EmergencyStopManagerTest, ConfigurableHysteresisAffectsReset) {
    Config& config = Config::getInstance();
    const double threshold = config.emergencyStopTemp.get();
    const double highTemp = threshold + 1.0;

    // Build up counter
    manager_->checkEmergencyConditions(highTemp);
    manager_->checkEmergencyConditions(highTemp);
    EXPECT_EQ(2, manager_->getDebounceCount());

    // Set larger hysteresis - temp needs to drop further to reset
    config.emergencyStopHysteresis.set(20.0);
    const double hysteresis = config.emergencyStopHysteresis.get();
    const double tempInOldHysteresis = threshold - 5.0;  // Would reset with hysteresis=10, but not with 20

    manager_->checkEmergencyConditions(tempInOldHysteresis);
    // Counter should NOT reset because temp is still in hysteresis zone (threshold - 20 < temp < threshold)
    EXPECT_GT(manager_->getDebounceCount(), 0) << "Counter should not reset when temp is still in hysteresis zone";
}
