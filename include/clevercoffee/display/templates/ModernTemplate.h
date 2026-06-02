/**
 * @file ModernTemplate.h
 * @brief Modern display template — idle, brew, and post-brew screens
 */

#pragma once

#include "clevercoffee/constants/Temperature.h"
#include "clevercoffee/coordinators/UICoordinator.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/display/DisplayLayoutUtils.h"
#include "clevercoffee/display/DisplayTemplateBase.h"
#include "clevercoffee/display/bitmaps.h"
#include "clevercoffee/display/displayHelpers.h"
#include "clevercoffee/display/languages.h"

#include <U8g2lib.h>

namespace ModernTemplateLayout {

// U8G2 uses setFontPosTop() — drawStr Y is the top of the glyph bounding box.
// Heights below are the font bbox pixel heights (not the number in the font name).
constexpr int kFontHeightFub20     = 23; // u8g2_font_fub20_tf
constexpr int kFontHeightProfont17 = 15; // u8g2_font_profont17_tf
constexpr int kFontHeightProfont11 = 11; // u8g2_font_profont11_tf
constexpr int kFontHeightProfont10 = 10; // u8g2_font_profont10_tf

constexpr int kStatusBarSeparatorY = STATUS_BAR_Y_POS;
constexpr int kRowGap              = 2;

constexpr int kIdleTempY    = kStatusBarSeparatorY + kRowGap;
constexpr int kIdleIconSize = 16;
constexpr int kIdleIconGap  = 4;

// Bottom strip (shared idle + brew): 10 px row at screen bottom; bar vertically centered with label.
constexpr int kBottomBarH        = 4;
constexpr int kBottomBarW        = 72;
constexpr int kBottomBarLabelGap = 3;
constexpr int kBottomRowH        = kFontHeightProfont10;
constexpr int kBottomRowY        = DISPLAY_HEIGHT - kBottomRowH;     // y=54..63
constexpr int kContentBottomY    = kBottomRowY - 1;                  // y=53

constexpr int kIdleStatusRowY = kContentBottomY - kIdleIconSize + 1; // icon y=38..53

constexpr int kBrewBarW = 88;

constexpr int kBrewMainY   = kStatusBarSeparatorY + kRowGap;
constexpr int kBrewFooterY = kContentBottomY - kFontHeightProfont10; // y=43..52

constexpr char kDegreeCProbe[]          = {'1', '0', '0', '.', '0', static_cast<char>(176), 'C', '\0'};
constexpr char kSetpointProbe[]         = {'1', '0', '0', static_cast<char>(176), 'C', '\0'};
constexpr char kBrewTargetTimeProbe[]   = {'/', ' ', '9', '9', '9', 's', '\0'};
constexpr char kBrewTargetWeightProbe[] = {'/', ' ', '9', '9', '9', 'g', '\0'};

constexpr int kTempMapMin = 20;

inline bool isReadyForBrew(const double tempC, const double setpointC) {
    return tempC >= setpointC - static_cast<double>(CleverCoffee::Temperature::HEATING_LOGO_THRESHOLD_C);
}

inline void drawHeatingIcon(U8G2* d, const int x, const int y) {
    d->drawTriangle(x + 8, y, x + 3, y + 12, x + 13, y + 12);
    d->drawBox(x + 5, y + 12, 6, 3);
    d->drawPixel(x + 8, y + 4);
}

inline void drawReadyIcon(U8G2* d, const int x, const int y) {
    d->drawFrame(x + 3, y + 2, 9, 8);
    d->drawLine(x + 12, y + 3, x + 15, y + 3);
    d->drawLine(x + 12, y + 7, x + 15, y + 7);
    d->drawLine(x + 4, y + 14, x + 11, y + 14);
}

inline int mapTempToBarWidth(const double tempC, const double setpointC, const int innerW) {
    if (setpointC <= static_cast<double>(kTempMapMin)) {
        return 0;
    }
    return constrain(map(static_cast<int>(tempC), kTempMapMin, static_cast<int>(setpointC), 0, innerW), 0, innerW);
}

inline void drawLargeTemperature(U8G2* d, const double temperatureC) {
    using CleverCoffee::Display::Layout::drawStrRightInBox;

    char tempBuf[12];
    snprintf(tempBuf, sizeof(tempBuf), "%.1f", temperatureC);

    char unitBuf[4];
    snprintf(unitBuf, sizeof(unitBuf), "%cC", 176);

    d->setFont(u8g2_font_fub20_tf);
    const int digitsBoxW = d->getStrWidth("100.0");

    d->setFont(u8g2_font_profont17_tf);
    const int unitW  = d->getStrWidth(unitBuf);
    const int unitY  = kIdleTempY + (kFontHeightFub20 - kFontHeightProfont17) / 2;
    const int totalW = digitsBoxW + kRowGap + unitW;
    const int startX = (DISPLAY_WIDTH - totalW) / 2;

    d->setFont(u8g2_font_fub20_tf);
    drawStrRightInBox(d, startX, digitsBoxW, kIdleTempY, tempBuf);

    d->setFont(u8g2_font_profont17_tf);
    d->drawStr(startX + digitsBoxW + kRowGap, unitY, unitBuf);
}

inline void drawTemperatureToSetpointBar(U8G2* d, const double currentTempC, const double setpointC) {
    if (setpointC <= static_cast<double>(kTempMapMin)) {
        return;
    }

    d->setFont(u8g2_font_profont10_tf);
    const auto layout = CleverCoffee::Display::Layout::layoutBarLabelCluster(d,
                                                                             kBottomBarW,
                                                                             kBottomBarH,
                                                                             kBottomRowY,
                                                                             kBottomRowH,
                                                                             kFontHeightProfont10,
                                                                             kBottomBarLabelGap,
                                                                             kSetpointProbe);

    d->drawFrame(layout.barX, layout.barY, kBottomBarW, kBottomBarH);

    const int innerW = kBottomBarW - 2;
    const int fillW  = mapTempToBarWidth(currentTempC, setpointC, innerW);
    if (fillW > 0) {
        d->drawBox(layout.barX + 1, layout.barY + 1, fillW, kBottomBarH - 2);
    }

    const double readyTempC = setpointC - static_cast<double>(CleverCoffee::Temperature::HEATING_LOGO_THRESHOLD_C);
    if (readyTempC > static_cast<double>(kTempMapMin)) {
        const int readyX = mapTempToBarWidth(readyTempC, setpointC, innerW);
        d->drawVLine(layout.barX + 1 + readyX, layout.barY - 1, kBottomBarH + 2);
    }

    char targetBuf[8];
    snprintf(targetBuf, sizeof(targetBuf), "%.0f%cC", setpointC, 176);
    CleverCoffee::Display::Layout::drawStrRightInBox(
        d, layout.labelX, d->getStrWidth(kSetpointProbe), layout.labelY, targetBuf);
}

} // namespace ModernTemplateLayout

class ModernTemplate : public DisplayTemplateBase<ModernTemplate> {
  public:
    /** No shared heating logo or fullscreen brew — idle/brew handled in renderNormalDisplay(). */
    using DisplayPolicy = CleverCoffee::Display::DisplayPolicy<false, false>;

    void renderNormalDisplay() {
        auto* d = systemContext_->hardwareContext().display();
        d->clearBuffer();

        const bool flushing = systemContext_->machineStateContext() &&
                              isManualFlushState(systemContext_->machineStateContext()->getCurrentStateId());

        const auto timerState = systemContext_->uiCoordinator().getBrewTimerDisplayState();

        if (flushing) {
            drawBrewScreen(d, true);
        } else if (timerState == CleverCoffee::UICoordinator::BrewTimerDisplayState::PostBrew) {
            drawPostBrewScreen(d);
        } else if (timerState == CleverCoffee::UICoordinator::BrewTimerDisplayState::Running) {
            drawBrewScreen(d, false);
        } else {
            drawIdleScreen(d);
        }
    }

    TemperatureCoords getTemperatureCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY, 84, baseX, baseY + 10, 84};
    }
    PIDCoords getPIDCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY, 96, baseY};
    }
    BrewCoords getBrewCoords(int baseX, int baseY) const noexcept {
        return {baseX, baseY};
    }
    const char* getCurrentTempLabel() const noexcept {
        return "";
    }
    const char* getSetTempLabel() const noexcept {
        return "";
    }
    const char* getBrewLabel() const noexcept {
        return langstring_brew;
    }
    const char* getManualFlushLabel() const noexcept {
        return langstring_manual_flush;
    }
    const char* getHotWaterLabel() const noexcept {
        return langstring_hot_water;
    }
    const char* getPIDSeparator() const noexcept {
        return "|";
    }

  private:
    enum class BrewPhase {
        PRE_INFUSION,
        BREWING,
        DONE
    };

    BrewPhase getCurrentPhase() const {
        if (!systemContext_->machineStateContext()) return BrewPhase::BREWING;

        const auto state = systemContext_->machineStateContext()->getCurrentStateId();
        if (state == MachineStateId::BREW_PREINFUSION || state == MachineStateId::BREW_PREINFUSION_PAUSE) {
            return BrewPhase::PRE_INFUSION;
        }
        if (state == MachineStateId::BREW_FINISHED) {
            return BrewPhase::DONE;
        }
        return BrewPhase::BREWING;
    }

    void drawPhaseIndicator(U8G2* d, BrewPhase phase, bool isFlushing) {
        d->setFont(u8g2_font_profont10_tf);

        if (isFlushing) {
            d->drawStr(2, 1, "FLUSHING");
        } else {
            const char* labels[] = {"PRE-INF", "BREW", "DONE"};
            const int   xs[]     = {2, 46, 86};
            const int   active   = phase == BrewPhase::PRE_INFUSION ? 0 : (phase == BrewPhase::BREWING ? 1 : 2);

            for (int i = 0; i < 3; ++i) {
                if (i > 0) {
                    d->drawStr(xs[i] - 8, 1, ">");
                }
                if (i == active) {
                    const int w = d->getStrWidth(labels[i]);
                    d->drawBox(xs[i] - 1, 0, w + 2, 10);
                    d->setDrawColor(0);
                    d->drawStr(xs[i], 1, labels[i]);
                    d->setDrawColor(1);
                } else {
                    d->drawStr(xs[i], 1, labels[i]);
                }
            }
        }

        d->drawLine(0, 12, 128, 12);
    }

    void drawBrewMainReadout(U8G2* d) {
        using CleverCoffee::Display::Layout::drawStrRightInBox;

        const int    seconds = static_cast<int>(systemContext_->processCurrentBrewTime() / 1000.0);
        const double tempC   = systemContext_->processTemperature();

        char timeBuf[12];
        snprintf(timeBuf, sizeof(timeBuf), "%d s", seconds);

        char tempBuf[12];
        snprintf(tempBuf, sizeof(tempBuf), "%.1f%cC", tempC, static_cast<char>(176));

        d->setFont(u8g2_font_profont17_tf);
        const int timeBoxW = d->getStrWidth("999 s");
        const int sepW     = d->getStrWidth(" - ");
        const int tempBoxW = d->getStrWidth(ModernTemplateLayout::kDegreeCProbe);
        const int blockW   = timeBoxW + sepW + tempBoxW;
        const int blockX   = (DISPLAY_WIDTH - blockW) / 2;
        const int y        = ModernTemplateLayout::kBrewMainY;

        drawStrRightInBox(d, blockX, timeBoxW, y, timeBuf);
        d->drawStr(blockX + timeBoxW, y, " - ");
        drawStrRightInBox(d, blockX + timeBoxW + sepW, tempBoxW, y, tempBuf);
    }

    void drawBrewProgressBar(U8G2* d) {
        using CleverCoffee::Display::Layout::drawStrRightInBox;
        using CleverCoffee::Display::Layout::layoutBarLabelCluster;

        const bool isAutomatic = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
        const bool brewByTime  = Config::getInstance().brewByTimeEnabled.get();
        const bool brewByWeight =
            Config::getInstance().brewByWeightEnabled.get() && Config::getInstance().hardwareSensorsScaleEnabled.get();

        int         fillWidth       = 0;
        bool        hasTarget       = false;
        char        targetLabel[16] = {};
        const char* labelProbe      = ModernTemplateLayout::kBrewTargetTimeProbe;

        if (isAutomatic && brewByWeight) {
            const double target  = Config::getInstance().brewByWeightTargetWeight.get();
            const double current = systemContext_->sensorCoordinator().getBrewWeight();
            if (target > 0) {
                const int percent = constrain(static_cast<int>(current / target * 100.0), 0, 100);
                fillWidth         = constrain(map(percent, 0, 100, 0, ModernTemplateLayout::kBrewBarW - 2),
                                      0,
                                      ModernTemplateLayout::kBrewBarW - 2);
                snprintf(targetLabel, sizeof(targetLabel), "/ %.0fg", target);
                hasTarget  = true;
                labelProbe = ModernTemplateLayout::kBrewTargetWeightProbe;
            }
        }

        if (!hasTarget && isAutomatic && brewByTime) {
            const double target  = systemContext_->processTotalTargetBrewTime();
            const double current = systemContext_->processCurrentBrewTime();
            if (target > 0) {
                const int percent = constrain(static_cast<int>(current / target * 100.0), 0, 100);
                fillWidth         = constrain(map(percent, 0, 100, 0, ModernTemplateLayout::kBrewBarW - 2),
                                      0,
                                      ModernTemplateLayout::kBrewBarW - 2);
                snprintf(targetLabel, sizeof(targetLabel), "/ %ds", static_cast<int>(target / 1000.0));
                hasTarget = true;
            }
        }

        d->setFont(u8g2_font_profont10_tf);

        CleverCoffee::Display::Layout::BarLabelCluster layout{};
        if (hasTarget) {
            layout = layoutBarLabelCluster(d,
                                           ModernTemplateLayout::kBrewBarW,
                                           ModernTemplateLayout::kBottomBarH,
                                           ModernTemplateLayout::kBottomRowY,
                                           ModernTemplateLayout::kBottomRowH,
                                           ModernTemplateLayout::kFontHeightProfont10,
                                           ModernTemplateLayout::kBottomBarLabelGap,
                                           labelProbe);
        } else {
            layout.barX = (DISPLAY_WIDTH - ModernTemplateLayout::kBrewBarW) / 2;
            layout.barY = ModernTemplateLayout::kBottomRowY +
                          (ModernTemplateLayout::kBottomRowH - ModernTemplateLayout::kBottomBarH) / 2;
        }

        d->drawFrame(layout.barX, layout.barY, ModernTemplateLayout::kBrewBarW, ModernTemplateLayout::kBottomBarH);

        if (hasTarget && fillWidth > 0) {
            d->drawBox(layout.barX + 1, layout.barY + 1, fillWidth, ModernTemplateLayout::kBottomBarH - 2);
        }

        if (hasTarget) {
            drawStrRightInBox(d, layout.labelX, d->getStrWidth(labelProbe), layout.labelY, targetLabel);
        }
    }

    void drawBrewFooter(U8G2* d) {
        d->setFont(u8g2_font_profont10_tf);
        int xPos = 4;

        if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
            char         weightBuf[16];
            const double weight = systemContext_->sensorCoordinator().getBrewWeight();

            const bool   isAutomatic  = Config::getInstance().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
            const bool   brewByWeight = Config::getInstance().brewByWeightEnabled.get();
            const double targetWeight = Config::getInstance().brewByWeightTargetWeight.get();

            if (isAutomatic && brewByWeight && targetWeight > 0) {
                snprintf(weightBuf, sizeof(weightBuf), "%.1fg/%.0fg", weight, targetWeight);
            } else {
                snprintf(weightBuf, sizeof(weightBuf), "%.1f g", weight);
            }
            d->drawStr(xPos, ModernTemplateLayout::kBrewFooterY, weightBuf);
            xPos += d->getStrWidth(weightBuf) + 10;
        }

        if (Config::getInstance().hardwareSensorsPressureEnabled.get()) {
            char pressureBuf[12];
            snprintf(pressureBuf, sizeof(pressureBuf), "%.1f bar", systemContext_->sensorCoordinator().getPressure());
            d->drawStr(xPos, ModernTemplateLayout::kBrewFooterY, pressureBuf);
        }
    }

    void drawPostBrewScreen(U8G2* d) {
        using CleverCoffee::Display::Layout::drawStrRightInBox;

        constexpr int cupX = (DISPLAY_WIDTH - Brew_Cup_Logo_width) / 2;
        constexpr int cupY = 2;
        d->drawXBMP(cupX, cupY, Brew_Cup_Logo_width, Brew_Cup_Logo_height, Brew_Cup_Logo);

        const auto brewTimeMs = static_cast<int>(systemContext_->processCurrentBrewTime());
        char       timeBuf[12];
        snprintf(timeBuf, sizeof(timeBuf), "%d.%d", brewTimeMs / 1000, (brewTimeMs % 1000) / 100);

        d->setFont(u8g2_font_profont17_tf);
        constexpr int timeY   = cupY + Brew_Cup_Logo_height + 4; // y=46, font 15px → ends y=60
        const int     digitsW = d->getStrWidth("999.9");
        const int     unitW   = d->getStrWidth(" s");
        const int     totalW  = digitsW + unitW;
        const int     startX  = (DISPLAY_WIDTH - totalW) / 2;

        drawStrRightInBox(d, startX, digitsW, timeY, timeBuf);
        d->drawStr(startX + digitsW, timeY, " s");
    }

    void drawBrewScreen(U8G2* d, bool isFlushing) {
        const BrewPhase phase = isFlushing ? BrewPhase::BREWING : getCurrentPhase();
        drawPhaseIndicator(d, phase, isFlushing);
        drawBrewMainReadout(d);
        if (!isFlushing) {
            drawBrewProgressBar(d);
        }
        drawBrewFooter(d);
    }

    void drawIdleScreen(U8G2* d) {
        displayStatusbar(*systemContext_);

        ModernTemplateLayout::drawLargeTemperature(d, systemContext_->processTemperature());
        drawIdleStatusRow(d);
        ModernTemplateLayout::drawTemperatureToSetpointBar(
            d, systemContext_->processTemperature(), systemContext_->processSetpoint());
    }

    void drawIdleStatusRow(U8G2* d) {
        if (!systemContext_->machineStateContext()) {
            return;
        }

        const double tempC      = systemContext_->processTemperature();
        const double setpointC  = systemContext_->processSetpoint();
        const bool   isReady    = ModernTemplateLayout::isReadyForBrew(tempC, setpointC);
        const bool   atSetpoint = CleverCoffee::Display::isNearSetpointForDisplay(tempC, setpointC);
        const bool   blinkOff   = isReady && atSetpoint && !CleverCoffee::Display::isBlinkPhaseOn(*systemContext_);
        const char*  status     = nullptr;

        if (isManualFlushState(systemContext_->machineStateContext()->getCurrentStateId())) {
            status = "FLUSHING";
        } else if (isReady) {
            status = "READY";
        } else {
            status = "HEATING";
        }

        if (blinkOff) {
            return;
        }

        d->setFont(u8g2_font_profont11_tf);
        const int y     = ModernTemplateLayout::kIdleStatusRowY;
        const int textY = y + (ModernTemplateLayout::kIdleIconSize - ModernTemplateLayout::kFontHeightProfont11) / 2;

        if (isManualFlushState(systemContext_->machineStateContext()->getCurrentStateId())) {
            const int textW = d->getStrWidth(status);
            d->drawStr((DISPLAY_WIDTH - textW) / 2, textY, status);
            return;
        }

        const int textW  = d->getStrWidth(status);
        const int totalW = ModernTemplateLayout::kIdleIconSize + ModernTemplateLayout::kIdleIconGap + textW;
        const int x      = (DISPLAY_WIDTH - totalW) / 2;

        if (isReady) {
            ModernTemplateLayout::drawReadyIcon(d, x, y);
        } else {
            ModernTemplateLayout::drawHeatingIcon(d, x, y);
        }

        d->drawStr(x + ModernTemplateLayout::kIdleIconSize + ModernTemplateLayout::kIdleIconGap, textY, status);
    }
};
