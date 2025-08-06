/**
 * @file CustomFormatters.h
 * @brief Custom std::format formatters for CleverCoffee types
 */

#pragma once

#include "../defaults.h"
#include "../state/GlobalState.h"

#if __cplusplus >= 202002L && __has_include(<format>)

#include <format>

/**
 * @brief Custom formatter for Hardware::SwitchType
 */
template <>
struct std::formatter<Hardware::SwitchType> {
    constexpr auto parse(format_parse_context& ctx) {
        return ctx.begin();
    }

    template <typename FormatContext>
    auto format(Hardware::SwitchType type, FormatContext& ctx) {
        switch (type) {
            case Hardware::SwitchType::MOMENTARY:
                return std::format_to(ctx.out(), "MOMENTARY");
            case Hardware::SwitchType::TOGGLE:
                return std::format_to(ctx.out(), "TOGGLE");
            default:
                return std::format_to(ctx.out(), "UNKNOWN_SWITCH({})", static_cast<int>(type));
        }
    }
};

/**
 * @brief Custom formatter for Hardware::SwitchMode
 */
template <>
struct std::formatter<Hardware::SwitchMode> {
    constexpr auto parse(format_parse_context& ctx) {
        return ctx.begin();
    }

    template <typename FormatContext>
    auto format(Hardware::SwitchMode mode, FormatContext& ctx) {
        switch (mode) {
            case Hardware::SwitchMode::NORMALLY_OPEN:
                return std::format_to(ctx.out(), "NORMALLY_OPEN");
            case Hardware::SwitchMode::NORMALLY_CLOSED:
                return std::format_to(ctx.out(), "NORMALLY_CLOSED");
            default:
                return std::format_to(ctx.out(), "UNKNOWN_MODE({})", static_cast<int>(mode));
        }
    }
};

/**
 * @brief Custom formatter for Hardware::RelayTriggerType
 */
template <>
struct std::formatter<Hardware::RelayTriggerType> {
    constexpr auto parse(format_parse_context& ctx) {
        return ctx.begin();
    }

    template <typename FormatContext>
    auto format(Hardware::RelayTriggerType trigger, FormatContext& ctx) {
        switch (trigger) {
            case Hardware::RelayTriggerType::LOW_TRIGGER:
                return std::format_to(ctx.out(), "LOW_TRIGGER");
            case Hardware::RelayTriggerType::HIGH_TRIGGER:
                return std::format_to(ctx.out(), "HIGH_TRIGGER");
            default:
                return std::format_to(ctx.out(), "UNKNOWN_TRIGGER({})", static_cast<int>(trigger));
        }
    }
};

/**
 * @brief Custom formatter for Hardware::TemperatureSensorType
 */
template <>
struct std::formatter<Hardware::TemperatureSensorType> {
    constexpr auto parse(format_parse_context& ctx) {
        return ctx.begin();
    }

    template <typename FormatContext>
    auto format(Hardware::TemperatureSensorType sensor, FormatContext& ctx) {
        switch (sensor) {
            case Hardware::TemperatureSensorType::TSIC_306:
                return std::format_to(ctx.out(), "TSIC_306");
            case Hardware::TemperatureSensorType::DALLAS_DS18B20:
                return std::format_to(ctx.out(), "DALLAS_DS18B20");
            default:
                return std::format_to(ctx.out(), "UNKNOWN_SENSOR({})", static_cast<int>(sensor));
        }
    }
};

/**
 * @brief Custom formatter for Hardware::ScaleType
 */
template <>
struct std::formatter<Hardware::ScaleType> {
    constexpr auto parse(format_parse_context& ctx) {
        return ctx.begin();
    }

    template <typename FormatContext>
    auto format(Hardware::ScaleType scale, FormatContext& ctx) {
        switch (scale) {
            case Hardware::ScaleType::HX711_DUAL:
                return std::format_to(ctx.out(), "HX711_DUAL");
            case Hardware::ScaleType::HX711_SINGLE:
                return std::format_to(ctx.out(), "HX711_SINGLE");
            case Hardware::ScaleType::BLUETOOTH:
                return std::format_to(ctx.out(), "BLUETOOTH");
            default:
                return std::format_to(ctx.out(), "UNKNOWN_SCALE({})", static_cast<int>(scale));
        }
    }
};

/**
 * @brief Custom formatter for System::DisplayTemplate
 */
template <>
struct std::formatter<System::DisplayTemplate> {
    constexpr auto parse(format_parse_context& ctx) {
        return ctx.begin();
    }

    template <typename FormatContext>
    auto format(System::DisplayTemplate tpl, FormatContext& ctx) {
        switch (tpl) {
            case System::DisplayTemplate::STANDARD:
                return std::format_to(ctx.out(), "STANDARD");
            case System::DisplayTemplate::MINIMAL:
                return std::format_to(ctx.out(), "MINIMAL");
            case System::DisplayTemplate::TEMPERATURE_ONLY:
                return std::format_to(ctx.out(), "TEMPERATURE_ONLY");
            case System::DisplayTemplate::SCALE:
                return std::format_to(ctx.out(), "SCALE");
            case System::DisplayTemplate::UPRIGHT:
                return std::format_to(ctx.out(), "UPRIGHT");
            default:
                return std::format_to(ctx.out(), "UNKNOWN_TEMPLATE({})", static_cast<int>(tpl));
        }
    }
};

/**
 * @brief Custom formatter for System::Language
 */
template <>
struct std::formatter<System::Language> {
    constexpr auto parse(format_parse_context& ctx) {
        return ctx.begin();
    }

    template <typename FormatContext>
    auto format(System::Language lang, FormatContext& ctx) {
        switch (lang) {
            case System::Language::ENGLISH:
                return std::format_to(ctx.out(), "ENGLISH");
            case System::Language::GERMAN:
                return std::format_to(ctx.out(), "GERMAN");
            case System::Language::SPANISH:
                return std::format_to(ctx.out(), "SPANISH");
            default:
                return std::format_to(ctx.out(), "UNKNOWN_LANGUAGE({})", static_cast<int>(lang));
        }
    }
};

/**
 * @brief Custom formatter for Process::BrewMode
 */
template <>
struct std::formatter<Process::BrewMode> {
    constexpr auto parse(format_parse_context& ctx) {
        return ctx.begin();
    }

    template <typename FormatContext>
    auto format(Process::BrewMode mode, FormatContext& ctx) {
        switch (mode) {
            case Process::BrewMode::MANUAL_BREW:
                return std::format_to(ctx.out(), "MANUAL");
            case Process::BrewMode::AUTOMATIC_BREW:
                return std::format_to(ctx.out(), "AUTOMATIC");
            default:
                return std::format_to(ctx.out(), "UNKNOWN_BREW_MODE({})", static_cast<int>(mode));
        }
    }
};

/**
 * @brief Custom formatter for machine states (from StateMachineStates.h)
 */
template <>
struct std::formatter<int> {
    bool is_machine_state = false;
    
    constexpr auto parse(format_parse_context& ctx) {
        auto it = ctx.begin(), end = ctx.end();
        if (it != end && *it == 'm') { // Format specifier 'm' for machine state
            is_machine_state = true;
            ++it;
        }
        return it;
    }

    template <typename FormatContext>
    auto format(int state, FormatContext& ctx) {
        if (!is_machine_state) {
            return std::format_to(ctx.out(), "{}", state);
        }
        
        // Format as machine state
        switch (state) {
            case kInit: return std::format_to(ctx.out(), "INIT");
            case kColdStart: return std::format_to(ctx.out(), "COLD_START");
            case kSetPointNegative: return std::format_to(ctx.out(), "SETPOINT_NEGATIVE");
            case kEmergencyStop: return std::format_to(ctx.out(), "EMERGENCY_STOP");
            case kPidNormal: return std::format_to(ctx.out(), "PID_NORMAL");
            case kBrewDetection: return std::format_to(ctx.out(), "BREW_DETECTION");
            case kBrew: return std::format_to(ctx.out(), "BREW");
            case kShotTimerExpired: return std::format_to(ctx.out(), "SHOT_TIMER_EXPIRED");
            case kBrewDetectionQM: return std::format_to(ctx.out(), "BREW_DETECTION_QM");
            case kPreInfusion: return std::format_to(ctx.out(), "PRE_INFUSION");
            case kWaitPreInfusion: return std::format_to(ctx.out(), "WAIT_PRE_INFUSION");
            case kPreInfusionPause: return std::format_to(ctx.out(), "PRE_INFUSION_PAUSE");
            case kWaitPreInfusionPause: return std::format_to(ctx.out(), "WAIT_PRE_INFUSION_PAUSE");
            case kBrewRunning: return std::format_to(ctx.out(), "BREW_RUNNING");
            case kWaitBrew: return std::format_to(ctx.out(), "WAIT_BREW");
            case kBrewFinished: return std::format_to(ctx.out(), "BREW_FINISHED");
            case kWaitBrewOff: return std::format_to(ctx.out(), "WAIT_BREW_OFF");
            case kSteam: return std::format_to(ctx.out(), "STEAM");
            case kWaitSteam: return std::format_to(ctx.out(), "WAIT_STEAM");
            case kCoolDown: return std::format_to(ctx.out(), "COOL_DOWN");
            case kBackflush: return std::format_to(ctx.out(), "BACKFLUSH");
            case kHotWater: return std::format_to(ctx.out(), "HOT_WATER");
            case kPidDisabled: return std::format_to(ctx.out(), "PID_DISABLED");
            case kSensorError: return std::format_to(ctx.out(), "SENSOR_ERROR");
            case kEepromError: return std::format_to(ctx.out(), "EEPROM_ERROR");
            case kServiceMode: return std::format_to(ctx.out(), "SERVICE_MODE");
            case kWaterTankEmpty: return std::format_to(ctx.out(), "WATER_TANK_EMPTY");
            case kManualFlush: return std::format_to(ctx.out(), "MANUAL_FLUSH");
            case kStandby: return std::format_to(ctx.out(), "STANDBY");
            default: return std::format_to(ctx.out(), "UNKNOWN_STATE({})", state);
        }
    }
};

/**
 * @brief Custom formatter for temperature with units
 */
struct Temperature {
    double value;
    const char* unit = "°C";
    
    constexpr Temperature(double val, const char* u = "°C") noexcept 
        : value(val), unit(u) {}
};

template <>
struct std::formatter<Temperature> {
    int precision = 1;
    
    constexpr auto parse(format_parse_context& ctx) {
        auto it = ctx.begin(), end = ctx.end();
        if (it != end && *it >= '0' && *it <= '9') {
            precision = *it - '0';
            ++it;
        }
        return it;
    }

    template <typename FormatContext>
    auto format(const Temperature& temp, FormatContext& ctx) {
        return std::format_to(ctx.out(), "{:.{}f}{}", temp.value, precision, temp.unit);
    }
};

/**
 * @brief Custom formatter for PID parameters
 */
struct PIDParams {
    double kp, ki, kd;
    
    constexpr PIDParams(double p, double i, double d) noexcept 
        : kp(p), ki(i), kd(d) {}
};

template <>
struct std::formatter<PIDParams> {
    int precision = 2;
    
    constexpr auto parse(format_parse_context& ctx) {
        auto it = ctx.begin(), end = ctx.end();
        if (it != end && *it >= '0' && *it <= '9') {
            precision = *it - '0';
            ++it;
        }
        return it;
    }

    template <typename FormatContext>
    auto format(const PIDParams& pid, FormatContext& ctx) {
        return std::format_to(ctx.out(), "Kp={:.{}f}, Ki={:.{}f}, Kd={:.{}f}", 
                             pid.kp, precision, pid.ki, precision, pid.kd, precision);
    }
};

/**
 * @brief Custom formatter for memory information
 */
struct MemoryInfo {
    size_t used, total, free, largest_block;
    
    constexpr MemoryInfo(size_t u, size_t t, size_t f, size_t lb) noexcept
        : used(u), total(t), free(f), largest_block(lb) {}
};

template <>
struct std::formatter<MemoryInfo> {
    bool show_percentage = true;
    
    constexpr auto parse(format_parse_context& ctx) {
        auto it = ctx.begin(), end = ctx.end();
        if (it != end && *it == 'n') { // 'n' for no percentage
            show_percentage = false;
            ++it;
        }
        return it;
    }

    template <typename FormatContext>
    auto format(const MemoryInfo& mem, FormatContext& ctx) {
        if (show_percentage) {
            double usage = (static_cast<double>(mem.used) / mem.total) * 100.0;
            return std::format_to(ctx.out(), "{}/{} bytes ({:.1f}% used), free: {}, largest: {}", 
                                 mem.used, mem.total, usage, mem.free, mem.largest_block);
        } else {
            return std::format_to(ctx.out(), "used: {}, free: {}, total: {}, largest: {}", 
                                 mem.used, mem.free, mem.total, mem.largest_block);
        }
    }
};

/**
 * @brief Custom formatter for WiFi signal strength with visual indicator
 */
struct WiFiSignal {
    int strength; // 0-4 scale
    bool connected;
    
    constexpr WiFiSignal(int s, bool c) noexcept : strength(s), connected(c) {}
};

template <>
struct std::formatter<WiFiSignal> {
    bool show_visual = true;
    
    constexpr auto parse(format_parse_context& ctx) {
        auto it = ctx.begin(), end = ctx.end();
        if (it != end && *it == 'n') { // 'n' for no visual
            show_visual = false;
            ++it;
        }
        return it;
    }

    template <typename FormatContext>
    auto format(const WiFiSignal& wifi, FormatContext& ctx) {
        if (!wifi.connected) {
            return std::format_to(ctx.out(), "DISCONNECTED");
        }
        
        if (show_visual) {
            const char* bars[] = {"▁", "▁▂", "▁▂▃", "▁▂▃▄", "▁▂▃▄▅"};
            return std::format_to(ctx.out(), "{}signal {} ({})", 
                                 bars[wifi.strength], wifi.strength, 
                                 wifi.strength == 4 ? "excellent" : 
                                 wifi.strength == 3 ? "good" :
                                 wifi.strength == 2 ? "fair" : "weak");
        } else {
            return std::format_to(ctx.out(), "strength: {} ({})", wifi.strength,
                                 wifi.strength == 4 ? "excellent" : 
                                 wifi.strength == 3 ? "good" :
                                 wifi.strength == 2 ? "fair" : "weak");
        }
    }
};

/**
 * @brief Convenience factory functions
 */
namespace CleverCoffee::Formatters {
    constexpr Temperature temp(double value, const char* unit = "°C") noexcept {
        return Temperature{value, unit};
    }
    
    constexpr PIDParams pid(double kp, double ki, double kd) noexcept {
        return PIDParams{kp, ki, kd};
    }
    
    constexpr MemoryInfo memory(size_t used, size_t total, size_t free, size_t largest) noexcept {
        return MemoryInfo{used, total, free, largest};
    }
    
    constexpr WiFiSignal wifi(int strength, bool connected = true) noexcept {
        return WiFiSignal{strength, connected};
    }
}

#endif // __cplusplus >= 202002L && __has_include(<format>)