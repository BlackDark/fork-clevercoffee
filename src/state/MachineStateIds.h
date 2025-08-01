/**
 * @file MachineStateIds.h
 * @brief Machine state identifier constants
 */

#pragma once

/**
 * @brief Machine state identifiers
 * 
 * These constants match the existing MachineState enum values
 * to ensure compatibility during the migration.
 */
namespace MachineStateIds {
    constexpr int INIT = 0;
    constexpr int PID_NORMAL = 20;
    constexpr int BREW = 30;
    constexpr int MANUAL_FLUSH = 35;
    constexpr int HOT_WATER = 40;
    constexpr int STEAM = 50;
    constexpr int BACKFLUSH = 60;
    constexpr int WATER_TANK_EMPTY = 70;
    constexpr int EMERGENCY_STOP = 80;
    constexpr int PID_DISABLED = 90;
    constexpr int STANDBY = 95;
    constexpr int SENSOR_ERROR = 100;
    constexpr int EEPROM_ERROR = 110;
}