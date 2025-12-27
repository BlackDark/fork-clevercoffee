#include <gtest/gtest.h>
#include "../../include/clevercoffee/utils/EmergencyTemperatureDetector.h"

// ==================== TEST CASES ====================

class EmergencyTemperatureTest : public ::testing::Test {
 protected:
  EmergencyTemperatureDetector detector{150.0f, 10.0f};
};

// Test 1: Triggers above threshold
TEST_F(EmergencyTemperatureTest, TriggersAboveThreshold) {
  detector.reset();
  // Need 3 consecutive readings above threshold
  EXPECT_FALSE(detector.checkEmergency(160.0f));  // 1st reading
  EXPECT_FALSE(detector.checkEmergency(160.0f));  // 2nd reading
  EXPECT_TRUE(detector.checkEmergency(160.0f));   // 3rd reading triggers
}

// Test 2: Allows normal temperature
TEST_F(EmergencyTemperatureTest, AllowsNormalTemperature) {
  detector.reset();
  EXPECT_FALSE(detector.checkEmergency(92.0f));
  EXPECT_FALSE(detector.checkEmergency(92.0f));
  EXPECT_FALSE(detector.checkEmergency(92.0f));
  EXPECT_FALSE(detector.checkEmergency(92.0f));
}

// Test 3: Debounces high temperature (requires 3 consecutive)
TEST_F(EmergencyTemperatureTest, DebouncessHighTemperature) {
  detector.reset();
  EXPECT_FALSE(detector.checkEmergency(160.0f));  // 1st
  EXPECT_FALSE(detector.checkEmergency(160.0f));  // 2nd
  // Drop back before 3rd - should reset debounce
  EXPECT_FALSE(detector.checkEmergency(92.0f));
  // Now we should need 3 more
  EXPECT_FALSE(detector.checkEmergency(160.0f));  // 1st again
  EXPECT_FALSE(detector.checkEmergency(160.0f));  // 2nd again
  EXPECT_TRUE(detector.checkEmergency(160.0f));   // 3rd triggers
}

// Test 4: Hysteresis prevents false recovery
TEST_F(EmergencyTemperatureTest, HysteresisPreventsFalseRecovery) {
  detector.reset();
  // Trigger emergency
  detector.checkEmergency(160.0f);
  detector.checkEmergency(160.0f);
  EXPECT_TRUE(detector.checkEmergency(160.0f));
  EXPECT_TRUE(detector.isEmergency());

  // Drop to threshold - still in hysteresis zone
  // Hysteresis = 10, so threshold - hysteresis = 140
  // At 145 we're still in the zone [140, 150]
  EXPECT_TRUE(detector.checkEmergency(145.0f));  // Still emergency
  EXPECT_TRUE(detector.isEmergency());

  // Must drop below threshold - hysteresis to reset
  EXPECT_FALSE(detector.checkEmergency(139.0f));  // Below 140, resets
  EXPECT_FALSE(detector.isEmergency());
}

// Test 5: Detects sensor disconnection (above valid range)
TEST_F(EmergencyTemperatureTest, DetectsSensorDisconnection) {
  detector.reset();
  // 200°C is above max valid (180°C)
  EXPECT_TRUE(detector.checkEmergency(200.0f));
}

// Test 6: Detects short circuit (below valid range)
TEST_F(EmergencyTemperatureTest, DetectsShortCircuit) {
  detector.reset();
  // -100°C is below min valid (-40°C)
  EXPECT_TRUE(detector.checkEmergency(-100.0f));
}

// Test 7: Respects configurable threshold
TEST_F(EmergencyTemperatureTest, RespectsConfigurableThreshold) {
  EmergencyTemperatureDetector custom_detector(120.0f, 5.0f);
  custom_detector.reset();

  // Below custom threshold
  EXPECT_FALSE(custom_detector.checkEmergency(100.0f));

  // Above custom threshold
  EXPECT_FALSE(custom_detector.checkEmergency(125.0f));
  EXPECT_FALSE(custom_detector.checkEmergency(125.0f));
  EXPECT_TRUE(custom_detector.checkEmergency(125.0f));
}

// Test 8: Resets debounce on state change
TEST_F(EmergencyTemperatureTest, ResetsDebounceOnStateChange) {
  detector.reset();
  // Start debouncing
  EXPECT_FALSE(detector.checkEmergency(160.0f));  // count=1
  EXPECT_FALSE(detector.checkEmergency(160.0f));  // count=2
  // Drop into normal range - resets debounce count
  EXPECT_FALSE(detector.checkEmergency(92.0f));   // count=0, exits emergency zone

  // Now another high reading - debounce should reset
  EXPECT_FALSE(detector.checkEmergency(160.0f));  // count=1 again
  EXPECT_FALSE(detector.checkEmergency(160.0f));  // count=2 again
  EXPECT_TRUE(detector.checkEmergency(160.0f));   // count=3, triggers
}

// Test 9: Edge case - exactly at threshold
TEST_F(EmergencyTemperatureTest, ExactlyAtThreshold) {
  detector.reset();
  // Exactly at threshold (150.0f) should NOT trigger until above
  EXPECT_FALSE(detector.checkEmergency(150.0f));
  EXPECT_FALSE(detector.checkEmergency(150.0f));
  EXPECT_FALSE(detector.checkEmergency(150.0f));
  EXPECT_FALSE(detector.isEmergency());

  // Just above should trigger with debounce
  EXPECT_FALSE(detector.checkEmergency(150.1f));
  EXPECT_FALSE(detector.checkEmergency(150.1f));
  EXPECT_TRUE(detector.checkEmergency(150.1f));
  EXPECT_TRUE(detector.isEmergency());
}

// Test 10: Multiple oscillations
TEST_F(EmergencyTemperatureTest, MultipleOscillations) {
  detector.reset();

  // First high reading sequence
  EXPECT_FALSE(detector.checkEmergency(160.0f));
  EXPECT_FALSE(detector.checkEmergency(160.0f));
  EXPECT_TRUE(detector.checkEmergency(160.0f));
  EXPECT_TRUE(detector.isEmergency());

  // Cool down within hysteresis zone (140-150)
  EXPECT_TRUE(detector.checkEmergency(145.0f));
  EXPECT_TRUE(detector.isEmergency());

  // Cool down below hysteresis zone
  EXPECT_FALSE(detector.checkEmergency(130.0f));
  EXPECT_FALSE(detector.isEmergency());

  // Warm up again - should require 3 reads again
  EXPECT_FALSE(detector.checkEmergency(160.0f));
  EXPECT_FALSE(detector.checkEmergency(160.0f));
  EXPECT_TRUE(detector.checkEmergency(160.0f));
  EXPECT_TRUE(detector.isEmergency());
}
