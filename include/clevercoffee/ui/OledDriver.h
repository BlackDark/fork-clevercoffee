/**
 * @file OledDriver.h
 * @brief OLED setup and I2C buffer flush (drawing lives in display/* templates)
 */

#pragma once

#include <U8g2lib.h>

class DisplayManager;

namespace CleverCoffee {
class SystemContext;
}

class OledDriver {
  public:
    explicit OledDriver(DisplayManager* displayManager, CleverCoffee::SystemContext* systemContext = nullptr);

    ~OledDriver() = default;

    OledDriver(const OledDriver&)            = delete;
    OledDriver& operator=(const OledDriver&) = delete;
    OledDriver(OledDriver&&)                 = default;
    OledDriver& operator=(OledDriver&&)      = default;

    bool initialize();
    void prepareDisplay();
    void forceUpdate();

    bool isBufferReady() const {
        return bufferReady_;
    }
    void setBufferReady(bool ready) {
        bufferReady_ = ready;
    }
    bool isUpdateRunning() const {
        return updateRunning_;
    }
    void setUpdateRunning(bool running) {
        updateRunning_ = running;
    }

  private:
    const u8g2_cb_t* getU8G2Rotation(int rotation);

    DisplayManager*              displayManager_;
    CleverCoffee::SystemContext* systemContext_;
    U8G2*                        u8g2_;
    bool                         initialized_;
    bool                         bufferReady_;
    bool                         updateRunning_;
};
