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
 *
 * Note: The interface provides both high-level methods for common operations
 * and low-level getDisplay() access for complex rendering. Future refactoring
 * should move more rendering logic into this interface to improve testability.
 */
class IDisplayManager {
  public:
    virtual ~IDisplayManager() = default;

    /**
     * @brief Get raw U8G2 pointer for compatibility with existing code
     * @return Pointer to U8G2 instance, or nullptr if not initialized
     * @note This method exposes implementation details for backward compatibility.
     *       Prefer using higher-level methods when possible.
     */
    virtual U8G2* getDisplay() const noexcept = 0;

    /**
     * @brief Check if display is successfully initialized
     * @return true if display is ready for use
     */
    virtual bool isInitialized() const noexcept = 0;

    // === High-level display operations for testability ===

    /**
     * @brief Set display power save mode
     * @param enabled true to enable power save (display off), false to wake
     */
    virtual void setPowerSave(bool enabled) noexcept = 0;

    /**
     * @brief Clear the display buffer
     */
    virtual void clear() noexcept = 0;

    /**
     * @brief Send buffer content to display
     */
    virtual void update() noexcept = 0;

    /**
     * @brief Draw a string at specified position
     * @param x X coordinate
     * @param y Y coordinate (baseline of text)
     * @param text Null-terminated string to draw
     * @return Width of the drawn string in pixels
     */
    virtual uint8_t drawString(int x, int y, const char* text) noexcept = 0;

    /**
     * @brief Set font for subsequent text operations
     * @param font Pointer to U8G2 font data
     */
    virtual void setFont(const uint8_t* font) noexcept = 0;
};
