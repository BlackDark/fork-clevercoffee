/**
 * @file LED.h
 *
 * @brief Interface that switches have to implement
 */

#pragma once

#include "../defaults.h"

/**
 * @file Switch.h Abstract interface class for a switch
 */
class Switch {
    public:
        /**
         * @brief Constructor for a new switch
         *
         * @param type Switch type
         * @param mode Operation mode
         */
        Switch(const Hardware::SwitchType type, const Hardware::SwitchMode mode) :
            type_(type), mode_(mode) {
        }

        Switch() = delete;
        virtual ~Switch() = default;

        virtual bool isPressed() = 0;
        virtual bool longPressDetected() = 0;

    protected:
        Hardware::SwitchType type_;
        Hardware::SwitchMode mode_;
};
