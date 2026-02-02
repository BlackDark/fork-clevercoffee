/**
 * @file IDisplayManager.h
 * @brief Interface for display management - enables testing with mock implementations
 */

#pragma once

#include <U8g2lib.h>

/**
 * @class IDisplayManager
 * @brief Abstract interface for display management operations
 *
 * This interface enables dependency injection and testing of components
 * that depend on display functionality without requiring actual hardware.
 */
class IDisplayManager {
  public:
    virtual ~IDisplayManager() = default;

    /**
     * @brief Get raw U8G2 pointer for compatibility with existing code
     * @return Pointer to U8G2 instance, or nullptr if not initialized
     */
    virtual U8G2* getDisplay() const noexcept = 0;

    /**
     * @brief Check if display is successfully initialized
     * @return true if display is ready for use
     */
    virtual bool isInitialized() const noexcept = 0;
};
