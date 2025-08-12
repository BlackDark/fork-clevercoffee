/**
 * @file IHardwareService.h
 * @brief Hardware service interface for dependency injection
 */

#pragma once

#include "../hardware/Relay.h"
#include "../hardware/Switch.h"

/**
 * @interface IHardwareService
 * @brief Interface for hardware operations
 * 
 * This interface abstracts hardware operations to reduce coupling to global state
 */
class IHardwareService {
public:
    virtual ~IHardwareService() = default;
    
    // Relay operations
    virtual Relay* getHeaterRelay() = 0;
    virtual Relay* getPumpRelay() = 0; 
    virtual Relay* getValveRelay() = 0;
    
    virtual void turnHeaterOn() = 0;
    virtual void turnHeaterOff() = 0;
    virtual void turnPumpOn() = 0;
    virtual void turnPumpOff() = 0;
    virtual void turnValveOn() = 0;
    virtual void turnValveOff() = 0;
    
    // Switch operations
    virtual Switch* getPowerSwitch() = 0;
    virtual Switch* getBrewSwitch() = 0;
    virtual Switch* getSteamSwitch() = 0;
    virtual Switch* getHotWaterSwitch() = 0;
    
    virtual bool isPowerSwitchPressed() = 0;
    virtual bool isBrewSwitchPressed() = 0;
    virtual bool isSteamSwitchPressed() = 0;
    virtual bool isHotWaterSwitchPressed() = 0;
    
    // LED operations (optional)
    virtual void setStatusLed(bool state) = 0;
    virtual void setBrewLed(bool state) = 0;
    virtual void setSteamLed(bool state) = 0;
};