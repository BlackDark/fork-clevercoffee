/**
 * @file pinmapping.h
 *
 * @brief Default GPIO pin mapping
 *
 */

#pragma once

#include <cstdint>

/**
 * Input Pins
 */

// Switches/Buttons
#define PIN_POWERSWITCH 39
#define PIN_BREWSWITCH  34
#define PIN_STEAMSWITCH 35
#define PIN_WATERSWITCH 36

#define PIN_ROTARY_DT  4 // Rotary encoder data pin
#define PIN_ROTARY_CLK 3 // Rotary encoder clock pin
#define PIN_ROTARY_SW  5 // Rotary encoder switch

// Sensors
#define PIN_TEMPSENSOR      16
#define PIN_WATERTANKSENSOR 23
#define PIN_HXDAT           32 // Brew scale data pin 1
#define PIN_HXDAT2          25 // Brew scale data pin 2
#define PIN_HXCLK           33 // Brew scale clock pin

/**
 * Output pins
 */

// Relays
#define PIN_VALVE  17
#define PIN_PUMP   27
#define PIN_HEATER 2

// LEDs
#define PIN_STATUSLED 26 // 25 works with logging // Moved from pin 26 (pin 26 had hardware issues)
#define PIN_BREWLED   19 // Working correctly
#define PIN_STEAMLED  1  // 32 works with logging // Moved from pin 1 (UART TX - conflicts with serial logging)

// Periphery
#define PIN_ZC 18 // Dimmer circuit Zero Crossing

/**
 * Bidirectional Pins
 */
#define PIN_I2CSCL 22
#define PIN_I2CSDA 21

// Compile-time pin validation functions
namespace PinValidation {
    constexpr int MAX_GPIO_PINS = 40;
    
    constexpr bool isValidPin(int pin) noexcept {
        return pin >= 0 && pin < MAX_GPIO_PINS;
    }
    
    constexpr bool isInputPin(int pin) noexcept {
        return pin == PIN_POWERSWITCH || pin == PIN_BREWSWITCH ||
               pin == PIN_STEAMSWITCH || pin == PIN_WATERSWITCH ||
               pin == PIN_ROTARY_DT || pin == PIN_ROTARY_CLK ||
               pin == PIN_ROTARY_SW || pin == PIN_TEMPSENSOR ||
               pin == PIN_WATERTANKSENSOR || pin == PIN_HXDAT ||
               pin == PIN_HXDAT2 || pin == PIN_HXCLK || pin == PIN_ZC;
    }
    
    constexpr bool isOutputPin(int pin) noexcept {
        return pin == PIN_VALVE || pin == PIN_PUMP || pin == PIN_HEATER ||
               pin == PIN_STATUSLED || pin == PIN_BREWLED || pin == PIN_STEAMLED;
    }
    
    constexpr bool isBidirectionalPin(int pin) noexcept {
        return pin == PIN_I2CSCL || pin == PIN_I2CSDA;
    }
    
    // Compile-time validation that all pins are within valid range
    static_assert(isValidPin(PIN_POWERSWITCH), "PIN_POWERSWITCH out of range");
    static_assert(isValidPin(PIN_BREWSWITCH), "PIN_BREWSWITCH out of range");
    static_assert(isValidPin(PIN_STEAMSWITCH), "PIN_STEAMSWITCH out of range");
    static_assert(isValidPin(PIN_WATERSWITCH), "PIN_WATERSWITCH out of range");
    static_assert(isValidPin(PIN_ROTARY_DT), "PIN_ROTARY_DT out of range");
    static_assert(isValidPin(PIN_ROTARY_CLK), "PIN_ROTARY_CLK out of range");
    static_assert(isValidPin(PIN_ROTARY_SW), "PIN_ROTARY_SW out of range");
    static_assert(isValidPin(PIN_TEMPSENSOR), "PIN_TEMPSENSOR out of range");
    static_assert(isValidPin(PIN_WATERTANKSENSOR), "PIN_WATERTANKSENSOR out of range");
    static_assert(isValidPin(PIN_HXDAT), "PIN_HXDAT out of range");
    static_assert(isValidPin(PIN_HXDAT2), "PIN_HXDAT2 out of range");
    static_assert(isValidPin(PIN_HXCLK), "PIN_HXCLK out of range");
    static_assert(isValidPin(PIN_VALVE), "PIN_VALVE out of range");
    static_assert(isValidPin(PIN_PUMP), "PIN_PUMP out of range");
    static_assert(isValidPin(PIN_HEATER), "PIN_HEATER out of range");
    static_assert(isValidPin(PIN_STATUSLED), "PIN_STATUSLED out of range");
    static_assert(isValidPin(PIN_BREWLED), "PIN_BREWLED out of range");
    static_assert(isValidPin(PIN_STEAMLED), "PIN_STEAMLED out of range");
    static_assert(isValidPin(PIN_ZC), "PIN_ZC out of range");
    static_assert(isValidPin(PIN_I2CSCL), "PIN_I2CSCL out of range");
    static_assert(isValidPin(PIN_I2CSDA), "PIN_I2CSDA out of range");
}
