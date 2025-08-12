/**
 * @file IDisplay.h
 * @brief Display interface for dependency injection
 */

#pragma once

#include <Arduino.h>

/**
 * @interface IDisplay
 * @brief Abstract interface for display operations
 * 
 * This interface allows display functions to be decoupled from the global state
 * by accepting a display abstraction instead of directly accessing g_state.hardware.display
 */
class IDisplay {
public:
    virtual ~IDisplay() = default;
    
    // Buffer operations
    virtual void clearBuffer() = 0;
    virtual void sendBuffer() = 0;
    
    // Text operations
    virtual void setFont(const uint8_t* font) = 0;
    virtual void setCursor(int x, int y) = 0;
    virtual void print(const char* text) = 0;
    virtual void print(const String& text) = 0;
    virtual void print(double value, int decimals = 2) = 0;
    virtual void drawStr(int x, int y, const char* text) = 0;
    virtual void drawUTF8(int x, int y, const char* text) = 0;
    
    // Geometric operations
    virtual void drawLine(int x1, int y1, int x2, int y2) = 0;
    virtual void drawPixel(int x, int y) = 0;
    virtual void drawVLine(int x, int y, int length) = 0;
    virtual void drawDisc(int x, int y, int radius) = 0;
    virtual void drawCircle(int x, int y, int radius) = 0;
    
    // Display properties
    virtual int getDisplayWidth() const = 0;
    virtual int getDisplayHeight() const = 0;
    virtual int getMaxCharHeight() const = 0;
    virtual int getUTF8Width(const char* text) const = 0;
};

/**
 * @class U8g2DisplayAdapter
 * @brief Adapter to make U8G2 displays compatible with IDisplay interface
 */
template<typename U8G2Display>
class U8g2DisplayAdapter : public IDisplay {
private:
    U8G2Display* display_;
    
public:
    explicit U8g2DisplayAdapter(U8G2Display* display) : display_(display) {}
    
    void clearBuffer() override { display_->clearBuffer(); }
    void sendBuffer() override { display_->sendBuffer(); }
    
    void setFont(const uint8_t* font) override { display_->setFont(font); }
    void setCursor(int x, int y) override { display_->setCursor(x, y); }
    void print(const char* text) override { display_->print(text); }
    void print(const String& text) override { display_->print(text); }
    void print(double value, int decimals) override { display_->print(value, decimals); }
    void drawStr(int x, int y, const char* text) override { display_->drawStr(x, y, text); }
    void drawUTF8(int x, int y, const char* text) override { display_->drawUTF8(x, y, text); }
    
    void drawLine(int x1, int y1, int x2, int y2) override { display_->drawLine(x1, y1, x2, y2); }
    void drawPixel(int x, int y) override { display_->drawPixel(x, y); }
    void drawVLine(int x, int y, int length) override { display_->drawVLine(x, y, length); }
    void drawDisc(int x, int y, int radius) override { display_->drawDisc(x, y, radius); }
    void drawCircle(int x, int y, int radius) override { display_->drawCircle(x, y, radius); }
    
    int getDisplayWidth() const override { return display_->getDisplayWidth(); }
    int getDisplayHeight() const override { return display_->getDisplayHeight(); }
    int getMaxCharHeight() const override { return display_->getMaxCharHeight(); }
    int getUTF8Width(const char* text) const override { return display_->getUTF8Width(text); }
};