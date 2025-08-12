/**
 * @file ServiceContainer.h
 * @brief Simple service container for dependency injection
 */

#pragma once

#include "IHardwareService.h"
#include "IProcessService.h"
#include "GlobalStateHardwareService.h"
#include "GlobalStateProcessService.h"
#include "../display/IDisplay.h"
#include "../display/DisplayContext.h"
#include <memory>

/**
 * @class ServiceContainer
 * @brief Simple dependency injection container
 * 
 * This class manages service instances and provides them to components
 * that need them, reducing direct coupling to global state
 */
class ServiceContainer {
private:
    std::unique_ptr<IHardwareService> hardwareService_;
    std::unique_ptr<IProcessService> processService_;
    std::unique_ptr<IDisplay> displayAdapter_;
    
    // Singleton instance
    static ServiceContainer* instance_;
    
    ServiceContainer() = default;
    
public:
    ~ServiceContainer() = default;
    
    // Singleton access
    static ServiceContainer& getInstance() {
        if (!instance_) {
            instance_ = new ServiceContainer();
        }
        return *instance_;
    }
    
    // Delete copy/move constructors
    ServiceContainer(const ServiceContainer&) = delete;
    ServiceContainer& operator=(const ServiceContainer&) = delete;
    ServiceContainer(ServiceContainer&&) = delete;
    ServiceContainer& operator=(ServiceContainer&&) = delete;
    
    /**
     * @brief Initialize services (typically called during system startup)
     */
    void initializeServices() {
        // Initialize with global state adapters
        hardwareService_ = std::make_unique<GlobalStateHardwareService>();
        processService_ = std::make_unique<GlobalStateProcessService>();
        
        // Display adapter will be set when display is available
        displayAdapter_ = nullptr;
    }
    
    /**
     * @brief Set display adapter (call after display is initialized)
     */
    template<typename U8G2Display>
    void setDisplayAdapter(U8G2Display* display) {
        displayAdapter_ = std::make_unique<U8g2DisplayAdapter<U8G2Display>>(display);
    }
    
    /**
     * @brief Get hardware service
     */
    IHardwareService* getHardwareService() const {
        return hardwareService_.get();
    }
    
    /**
     * @brief Get process service  
     */
    IProcessService* getProcessService() const {
        return processService_.get();
    }
    
    /**
     * @brief Get display interface
     */
    IDisplay* getDisplay() const {
        return displayAdapter_.get();
    }
    
    /**
     * @brief Create display context with current process and network data
     */
    DisplayContext createDisplayContext() const {
        if (!displayAdapter_ || !processService_) {
            // Return empty context if services not available
            return DisplayContext(nullptr);
        }
        
        // Create process data from current service state
        ProcessData processData(
            processService_->getCurrentTemperature(),
            processService_->getSetpoint()
        );
        
        // For network data, we would need additional services
        // For now, return empty network data
        NetworkData networkData;
        
        return DisplayContext(displayAdapter_.get(), processData, networkData);
    }
    
    /**
     * @brief Reset all services (useful for testing)
     */
    void reset() {
        hardwareService_.reset();
        processService_.reset();
        displayAdapter_.reset();
    }
};

// Static member definition
ServiceContainer* ServiceContainer::instance_ = nullptr;