/**
 * @file BootDiagnostics.h
 * @brief Boot reset reason and last-panic crash-info strings (native-safe header)
 */

#pragma once

#include <cstddef>

namespace CleverCoffee {

/** Mirrors `esp_reset_reason_t` integers; no IDF types in this header. */
enum class ResetReason : int {
    Unknown           = 0,
    PowerOn           = 1,
    ExternalPin       = 2,
    Software          = 3,
    Panic             = 4,
    InterruptWatchdog = 5,
    TaskWatchdog      = 6,
    OtherWatchdog     = 7,
    DeepSleep         = 8,
    Brownout          = 9,
    Sdio              = 10
};

/** Maps `esp_reset_reason_t` integer values; no IDF types in this header. */
[[nodiscard]] inline const char* resetReasonToString(int reason) noexcept {
    switch (static_cast<ResetReason>(reason)) {
        case ResetReason::Unknown:
            return "unknown";
        case ResetReason::PowerOn:
            return "power-on";
        case ResetReason::ExternalPin:
            return "external reset pin";
        case ResetReason::Software:
            return "software restart";
        case ResetReason::Panic:
            return "panic or unhandled exception";
        case ResetReason::InterruptWatchdog:
            return "interrupt watchdog";
        case ResetReason::TaskWatchdog:
            return "task watchdog";
        case ResetReason::OtherWatchdog:
            return "other watchdog";
        case ResetReason::DeepSleep:
            return "wake from deep sleep";
        case ResetReason::Brownout:
            return "brownout";
        case ResetReason::Sdio:
            return "SDIO";
        default:
            return "unknown";
    }
}

/**
 * Captured once at boot. crashInfo is the last stored panic, not necessarily this boot —
 * pair with resetReason. Native tests may call the getters without linking capture().
 */
class BootDiagnostics {
  public:
    static void capture();

    [[nodiscard]] static const char* resetReasonString() noexcept {
        return resetReasonToString(resetReason_);
    }

    [[nodiscard]] static const char* crashInfoString() noexcept {
        return crashInfo_[0] != '\0' ? crashInfo_ : "none";
    }

  private:
    static constexpr std::size_t kCrashInfoSize = 176;

    static inline int  resetReason_{0};
    static inline char crashInfo_[kCrashInfoSize]{""};
};

} // namespace CleverCoffee
