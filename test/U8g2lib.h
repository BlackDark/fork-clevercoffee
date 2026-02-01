/**
 * @file U8g2lib.h
 * @brief U8g2lib stub for native test environment
 * 
 * Provides minimal U8G2 class stub for testing without actual display hardware
 */

#pragma once

// Forward declaration - minimal stub for U8G2 display class
class U8G2 {
public:
    U8G2() = default;
    virtual ~U8G2() = default;
    
    // Minimal interface - add methods as needed for tests
    void begin() {}
    void clearBuffer() {}
    void sendBuffer() {}
};

// Type alias used in code
typedef U8G2 U8G2;
