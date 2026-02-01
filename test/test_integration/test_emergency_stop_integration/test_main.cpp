/**
 * @file test_main.cpp
 * @brief Integration tests for emergency stop
 */

#include <gtest/gtest.h>
#include "../../test_support.h"

class EmergencyStopIntegrationTest : public ::testing::Test {
protected:
    void SetUp() override {}
};

TEST_F(EmergencyStopIntegrationTest, DISABLED_EmergencyStopDuringBrew) {
    // Test emergency stop during various states
}

// Note: main() is provided by test/main.cpp
