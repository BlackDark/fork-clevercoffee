/**
 * @file DisplayTemplatePolicy.h
 * @brief Per-template policy for shared pipeline stages (CRTP DisplayPolicy on each template)
 *
 * Helpers in displayHelpers.h stay template-agnostic (thresholds, near-setpoint only).
 * Each template selects which shared fullscreen/system stages run before renderNormalDisplay().
 */

#pragma once

namespace CleverCoffee::Display {

struct DefaultDisplayPolicy {
    static constexpr bool sharedHeatingLogoScreen() {
        return true;
    }
    static constexpr bool sharedFullscreenBrewTimer() {
        return true;
    }
    static constexpr bool sharedFullscreenManualFlushTimer() {
        return true;
    }
    static constexpr bool sharedFullscreenHotWaterTimer() {
        return true;
    }
};

/**
 * @tparam SharedHeatingLogo Shared fullscreen heating logo when > HEATING_LOGO_THRESHOLD_C below setpoint
 * @tparam SharedFullscreenBrew Shared fullscreen brew timer (config must also be enabled)
 * @tparam SharedFullscreenManualFlush Shared fullscreen manual flush timer
 * @tparam SharedFullscreenHotWater Shared fullscreen hot-water timer
 */
template <bool SharedHeatingLogo           = true,
          bool SharedFullscreenBrew        = true,
          bool SharedFullscreenManualFlush = true,
          bool SharedFullscreenHotWater    = true>
struct DisplayPolicy {
    static constexpr bool sharedHeatingLogoScreen() {
        return SharedHeatingLogo;
    }
    static constexpr bool sharedFullscreenBrewTimer() {
        return SharedFullscreenBrew;
    }
    static constexpr bool sharedFullscreenManualFlushTimer() {
        return SharedFullscreenManualFlush;
    }
    static constexpr bool sharedFullscreenHotWaterTimer() {
        return SharedFullscreenHotWater;
    }
};

} // namespace CleverCoffee::Display
