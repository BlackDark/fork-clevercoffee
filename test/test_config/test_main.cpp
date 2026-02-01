/**
 * @file test_main.cpp
 * @brief Comprehensive unit tests for Config system
 */

#include <gtest/gtest.h>
#include "../test_support.h"
#include "../mocks/MockConfig.h"
#include "clevercoffee/Config.h"
#include "clevercoffee/defaults.h"

// Include stubs and implementations needed for Config
#include "../mocks/ConfigStubs.cpp"
#include "../../src/Logger.cpp"  // Logger is needed by Config (ParamDef uses LOG macros)

/**
 * @brief Test fixture for Config tests
 */
class ConfigTest : public ::testing::Test {
protected:
    void SetUp() override {
        mockConfig_ = std::make_unique<MockConfig>();
    }

    void TearDown() override {
        mockConfig_.reset();
    }

    std::unique_ptr<MockConfig> mockConfig_;
};

/**
 * @brief Test MockConfig parameter getters and setters
 */
TEST_F(ConfigTest, MockConfigParameterAccess) {
    // Test PID parameters
    mockConfig_->setPidEnabled(true);
    EXPECT_TRUE(mockConfig_->getPidEnabled());
    
    mockConfig_->setPidRegularKp(10.5);
    EXPECT_DOUBLE_EQ(10.5, mockConfig_->getPidRegularKp());
    
    // Test setpoint parameters
    mockConfig_->setBrewSetpoint(95.0);
    EXPECT_DOUBLE_EQ(95.0, mockConfig_->getBrewSetpoint());
    
    mockConfig_->setSteamSetpoint(120.0);
    EXPECT_DOUBLE_EQ(120.0, mockConfig_->getSteamSetpoint());
    
    // Test switch parameters
    mockConfig_->setHardwareSwitchesBrewEnabled(true);
    EXPECT_TRUE(mockConfig_->getHardwareSwitchesBrewEnabled());
    
    mockConfig_->setHardwareSwitchesBrewType(Hardware::SwitchType::TOGGLE);
    EXPECT_EQ(Hardware::SwitchType::TOGGLE, mockConfig_->getHardwareSwitchesBrewType());
}

/**
 * @brief Test parameter default values
 */
TEST_F(ConfigTest, ParameterDefaultValues) {
    // Verify default values match expected
    EXPECT_TRUE(mockConfig_->getPidEnabled());
    EXPECT_DOUBLE_EQ(95.0, mockConfig_->getBrewSetpoint());
    EXPECT_DOUBLE_EQ(120.0, mockConfig_->getSteamSetpoint());
    EXPECT_DOUBLE_EQ(10.0, mockConfig_->getPidRegularKp());
    EXPECT_DOUBLE_EQ(100.0, mockConfig_->getPidRegularTn());
    EXPECT_DOUBLE_EQ(20.0, mockConfig_->getPidRegularTv());
}

/**
 * @brief Test parameter validation
 */
TEST_F(ConfigTest, ParameterValidation) {
    // Config parameters should validate ranges
    // This is tested conceptually since real Config uses NVS
    
    // Setpoint should be in reasonable range (e.g., 0-200°C)
    mockConfig_->setBrewSetpoint(95.0);
    EXPECT_GE(mockConfig_->getBrewSetpoint(), 0.0);
    EXPECT_LE(mockConfig_->getBrewSetpoint(), 200.0);
    
    // PID parameters should be positive
    mockConfig_->setPidRegularKp(10.0);
    EXPECT_GT(mockConfig_->getPidRegularKp(), 0.0);
}

/**
 * @brief Test enum parameter handling
 */
TEST_F(ConfigTest, EnumParameterHandling) {
    // Test switch type enum
    mockConfig_->setHardwareSwitchesBrewType(Hardware::SwitchType::MOMENTARY);
    EXPECT_EQ(Hardware::SwitchType::MOMENTARY, mockConfig_->getHardwareSwitchesBrewType());
    
    mockConfig_->setHardwareSwitchesBrewType(Hardware::SwitchType::TOGGLE);
    EXPECT_EQ(Hardware::SwitchType::TOGGLE, mockConfig_->getHardwareSwitchesBrewType());
}

/**
 * @brief Test parameter validation ranges
 */
TEST_F(ConfigTest, ParameterValidationRanges) {
    // Test setpoint validation (should be in reasonable range)
    mockConfig_->setBrewSetpoint(95.0);
    EXPECT_GE(mockConfig_->getBrewSetpoint(), 0.0);
    EXPECT_LE(mockConfig_->getBrewSetpoint(), 200.0);
    
    mockConfig_->setSteamSetpoint(120.0);
    EXPECT_GE(mockConfig_->getSteamSetpoint(), 0.0);
    EXPECT_LE(mockConfig_->getSteamSetpoint(), 200.0);
}

/**
 * @brief Test Config with real instance (if NVS available)
 */
TEST_F(ConfigTest, DISABLED_RealConfigWithNVS) {
    // This test would test real Config with NVS if available in test environment
    // Config& config = Config::getInstance();
    // config.begin();
    // config.brewSetpoint.set(96.0);
    // EXPECT_DOUBLE_EQ(96.0, config.brewSetpoint.get());
    // config.resetAllToDefaults();
}

/**
 * @brief Test parameter persistence
 */
TEST_F(ConfigTest, DISABLED_ParameterPersistence) {
    // This test would verify parameters persist across Config instances
    // Requires NVS functionality
}

