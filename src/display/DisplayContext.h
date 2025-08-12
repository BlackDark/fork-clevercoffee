/**
 * @file DisplayContext.h
 * @brief Display context for dependency injection
 */

#pragma once

#include "IDisplay.h"
#include "../Config.h"

class MQTTManager;
class CleverCoffeeWiFiManager;

/**
 * @struct ProcessData
 * @brief Process-related data needed for display functions
 */
struct ProcessData {
    double temperature = 0.0;
    double setpoint = 0.0;
    
    ProcessData() = default;
    ProcessData(double temp, double sp) : temperature(temp), setpoint(sp) {}
};

/**
 * @struct NetworkData  
 * @brief Network-related data needed for display functions
 */
struct NetworkData {
    MQTTManager* mqttManager = nullptr;
    CleverCoffeeWiFiManager* wifiManager = nullptr;
    
    NetworkData() = default;
    NetworkData(MQTTManager* mqtt, CleverCoffeeWiFiManager* wifi) 
        : mqttManager(mqtt), wifiManager(wifi) {}
};

/**
 * @class DisplayContext
 * @brief Context object containing all dependencies needed for display operations
 * 
 * This class encapsulates the data and interfaces needed by display functions,
 * reducing their coupling to global state by providing a clean injection point.
 */
class DisplayContext {
private:
    IDisplay* display_;
    ProcessData processData_;
    NetworkData networkData_;
    
public:
    DisplayContext(IDisplay* display, const ProcessData& process = {}, const NetworkData& network = {})
        : display_(display), processData_(process), networkData_(network) {}
    
    // Display interface access
    IDisplay* getDisplay() const { return display_; }
    
    // Process data access
    const ProcessData& getProcessData() const { return processData_; }
    void setProcessData(const ProcessData& data) { processData_ = data; }
    
    // Network data access  
    const NetworkData& getNetworkData() const { return networkData_; }
    void setNetworkData(const NetworkData& data) { networkData_ = data; }
    
    // Convenience methods for common operations
    void setFont(const uint8_t* font) { display_->setFont(font); }
    void setCursor(int x, int y) { display_->setCursor(x, y); }
    void print(const char* text) { display_->print(text); }
    void drawUTF8(int x, int y, const char* text) { display_->drawUTF8(x, y, text); }
    void clearBuffer() { display_->clearBuffer(); }
    void sendBuffer() { display_->sendBuffer(); }
};