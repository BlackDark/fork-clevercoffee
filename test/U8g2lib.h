/**
 * @file U8g2lib.h
 * @brief U8g2lib stub for native test environment
 *
 * Provides minimal U8G2 class stub for testing without actual display hardware
 */

#pragma once

#include <cstdint>

// U8G2 rotation constants
struct u8g2_cb_t {};
const u8g2_cb_t* const U8G2_R0 = nullptr;
const u8g2_cb_t* const U8G2_R1 = nullptr;
const u8g2_cb_t* const U8G2_R2 = nullptr;
const u8g2_cb_t* const U8G2_R3 = nullptr;

// U8G2 fonts (stub pointers)
const uint8_t* const u8g2_font_profont10_tf = nullptr;
const uint8_t* const u8g2_font_profont11_tf = nullptr;
const uint8_t* const u8g2_font_profont12_tf = nullptr;
const uint8_t* const u8g2_font_profont15_tf = nullptr;
const uint8_t* const u8g2_font_profont22_tf = nullptr;
const uint8_t* const u8g2_font_fub17_tf = nullptr;
const uint8_t* const u8g2_font_fub20_tf = nullptr;
const uint8_t* const u8g2_font_fub25_tf = nullptr;
const uint8_t* const u8g2_font_fub30_tf = nullptr;

// Forward declaration - minimal stub for U8G2 display class
class U8G2 {
public:
    U8G2() = default;
    virtual ~U8G2() = default;

    // Minimal interface - add methods as needed for tests
    void begin() {}
    void clearBuffer() {}
    void sendBuffer() {}
    void drawStr(int x, int y, const char* str) {}
    void setFont(const uint8_t* font) {}
    void drawXBMP(int x, int y, int w, int h, const uint8_t* bitmap) {}
    void drawVLine(int x, int y, int h) {}
    void setCursor(int x, int y) {}
    void print(const char* str) {}
    void print(int value) {}
    void print(unsigned int value) {}
    void print(long value) {}
    void print(unsigned long value) {}
    void print(double value, int decimals = 2) {}
    void drawLine(int x1, int y1, int x2, int y2) {}
    void drawPixel(int x, int y) {}
    void drawDisc(int x, int y, int r, uint8_t options = 0) {}
    void drawCircle(int x, int y, int r, uint8_t options = 0) {}
    void setDrawColor(int color) {}
    void drawBox(int x, int y, int w, int h) {}
    void drawFrame(int x, int y, int w, int h) {}
    int getUTF8Width(const char* str) { return 0; }
    void drawUTF8(int x, int y, const char* str) {}
    int getMaxCharHeight() { return 10; }
    int getDisplayWidth() { return 128; }
    int getDisplayHeight() { return 64; }
    void setPowerSave(int mode) {}
};

// Type alias used in code
typedef U8G2 U8G2;
