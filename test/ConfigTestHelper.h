/**
 * @file ConfigTestHelper.h
 * @brief Helper to reset Config singleton between tests
 *
 * For legacy test code that accesses Config::getInstance() directly, this
 * header provides `resetConfigDefaults()` which resets every public
 * ParamDef / EnumParamDef member back to its compiled-in default.
 *
 * New tests should prefer MockConfig (which implements IConfig) and avoid
 * the singleton entirely.
 *
 * Usage:
 * @code
 * #include "ConfigTestHelper.h"
 *
 * class MyTest : public ::testing::Test {
 * protected:
 *     void TearDown() override {
 *         resetConfigDefaults();
 *     }
 * };
 * @endcode
 *
 * @note This works because ParamDef::resetToDefault() and
 *       EnumParamDef::resetToDefault() are inline virtual methods defined in
 *       Config.h — no Config.cpp linkage is required.
 */

#pragma once

#include "clevercoffee/Config.h"

/**
 * @brief Reset all Config singleton parameters to their compiled-in defaults
 *
 * Calls resetToDefault() on every public ParamDef and EnumParamDef member of
 * Config::getInstance(). This is an inline function so it compiles in the
 * native_test environment without linking Config.cpp.
 */
inline void resetConfigDefaults() {
    Config& c = Config::getInstance();

    // ── PID ─────────────────────────────────────────────────────────
    c.pidEnabled.resetToDefault();
    c.pidUsePonm.resetToDefault();
    c.pidEmaFactor.resetToDefault();
    c.pidRegularKp.resetToDefault();
    c.pidRegularTn.resetToDefault();
    c.pidRegularTv.resetToDefault();
    c.pidRegularIMax.resetToDefault();
    c.pidSteamKp.resetToDefault();
    c.pidBdEnabled.resetToDefault();
    c.brewPidDelay.resetToDefault();
    c.pidBdKp.resetToDefault();
    c.pidBdTn.resetToDefault();
    c.pidBdTv.resetToDefault();

    // ── Brew ────────────────────────────────────────────────────────
    c.brewSetpoint.resetToDefault();
    c.brewTempOffset.resetToDefault();
    c.steamSetpoint.resetToDefault();
    c.emergencyStopTemp.resetToDefault();
    c.emergencyStopHysteresis.resetToDefault();
    c.brewByTimeEnabled.resetToDefault();
    c.brewByTimeTargetTime.resetToDefault();
    c.brewByWeightEnabled.resetToDefault();
    c.brewByWeightTargetWeight.resetToDefault();
    c.brewByWeightAutoTare.resetToDefault();
    c.brewPreInfusionEnabled.resetToDefault();
    c.brewPreInfusionTime.resetToDefault();
    c.brewPreInfusionPause.resetToDefault();
    c.brewMode.resetToDefault();

    // ── Hardware OLED ───────────────────────────────────────────────
    c.hardwareOledEnabled.resetToDefault();
    c.hardwareOledType.resetToDefault();
    c.hardwareOledAddress.resetToDefault();

    // ── Hardware Relays ─────────────────────────────────────────────
    c.hardwareRelaysHeaterTriggerType.resetToDefault();
    c.hardwareRelaysValveTriggerType.resetToDefault();
    c.hardwareRelaysPumpTriggerType.resetToDefault();

    // ── Hardware Switches ───────────────────────────────────────────
    c.hardwareSwitchesBrewEnabled.resetToDefault();
    c.hardwareSwitchesBrewType.resetToDefault();
    c.hardwareSwitchesBrewMode.resetToDefault();
    c.hardwareSwitchesSteamEnabled.resetToDefault();
    c.hardwareSwitchesSteamType.resetToDefault();
    c.hardwareSwitchesSteamMode.resetToDefault();
    c.hardwareSwitchesPowerEnabled.resetToDefault();
    c.hardwareSwitchesPowerType.resetToDefault();
    c.hardwareSwitchesPowerMode.resetToDefault();
    c.hardwareSwitchesHotWaterEnabled.resetToDefault();
    c.hardwareSwitchesHotWaterType.resetToDefault();
    c.hardwareSwitchesHotWaterMode.resetToDefault();

    // ── Hardware LEDs ───────────────────────────────────────────────
    c.hardwareLedsStatusEnabled.resetToDefault();
    c.hardwareLedsStatusInverted.resetToDefault();
    c.hardwareLedsBrewEnabled.resetToDefault();
    c.hardwareLedsBrewInverted.resetToDefault();
    c.hardwareLedsSteamEnabled.resetToDefault();
    c.hardwareLedsSteamInverted.resetToDefault();

    // ── Hardware Sensors ────────────────────────────────────────────
    c.hardwareSensorsTemperatureType.resetToDefault();
    c.hardwareSensorsPressureEnabled.resetToDefault();
    c.hardwareSensorsWatertankEnabled.resetToDefault();
    c.hardwareSensorsWatertankMode.resetToDefault();
    c.hardwareSensorsScaleEnabled.resetToDefault();
    c.hardwareSensorsScaleSamples.resetToDefault();
    c.hardwareSensorsScaleType.resetToDefault();
    c.hardwareSensorsScaleCalibration.resetToDefault();
    c.hardwareSensorsScaleCalibration2.resetToDefault();
    c.hardwareSensorsScaleKnownWeight.resetToDefault();

    // ── Display ─────────────────────────────────────────────────────
    c.displayTemplate.resetToDefault();
    c.displayInverted.resetToDefault();
    c.displayLanguage.resetToDefault();
    c.displayFullscreenBrewTimer.resetToDefault();
    c.displayFullscreenManualFlushTimer.resetToDefault();
    c.displayFullscreenHotWaterTimer.resetToDefault();
    c.displayPostBrewTimerDuration.resetToDefault();
    c.displayHeatingLogo.resetToDefault();
    c.displayPidOffLogo.resetToDefault();

    // ── Backflush ───────────────────────────────────────────────────
    c.backflushCycles.resetToDefault();
    c.backflushFillTime.resetToDefault();
    c.backflushFlushTime.resetToDefault();

    // ── Standby ─────────────────────────────────────────────────────
    c.standbyEnabled.resetToDefault();
    c.standbyTime.resetToDefault();

    // ── MQTT ────────────────────────────────────────────────────────
    c.mqttEnabled.resetToDefault();
    c.mqttBroker.resetToDefault();
    c.mqttPort.resetToDefault();
    c.mqttUsername.resetToDefault();
    c.mqttPassword.resetToDefault();
    c.mqttTopic.resetToDefault();
    c.mqttHassioEnabled.resetToDefault();
    c.mqttHassioPrefix.resetToDefault();

    // ── System ──────────────────────────────────────────────────────
    c.systemHostname.resetToDefault();
    c.systemOtaPassword.resetToDefault();
    c.systemOfflineMode.resetToDefault();
    c.systemLogLevel.resetToDefault();
    c.systemAuthEnabled.resetToDefault();
    c.systemAuthUsername.resetToDefault();
    c.systemAuthPassword.resetToDefault();
    c.systemTimingDebugEnabled.resetToDefault();
    c.systemShowdisplayEnabled.resetToDefault();
}
