/**
 * @file defaults.h
 *
 * @brief Default values for system parameters
 *
 */

#pragma once

// system parameter defaults and ranges

// default parameters
#define STORAGE_NAMESPACE "config"       // NVS namespace for storing parameters
#define HOSTNAME          "silvia"       // default hostname
#define OTAPASS           "otapass"      // default password for over-the-air updates
#define WM_PASS           "CleverCoffee" // default password for WiFiManager
// Numeric defaults - converted to constexpr for type safety
constexpr double SETPOINT                 = 95.0;   // brew temperature setpoint
constexpr double TEMPOFFSET               = 0.0;    // brew temperature offset
constexpr double STEAMSETPOINT            = 120.0;  // steam temperature setpoint
constexpr double SCALE_CALIBRATION_FACTOR = 1.00;   // Raw data is divided by this value to convert to readable data
constexpr double SCALE_KNOWN_WEIGHT       = 267.00; // Calibration weight for scale (weight of the tray)
constexpr int    SCALE_SAMPLES            = 2;      // Number of samples used for calibration
constexpr double AGGKP                    = 62.0;   // PID Kp (regular phase)
constexpr double AGGTN                    = 52.0;   // PID Tn (regular phase)
constexpr double AGGTV                    = 11.5;   // PID Tv (regular phase)
constexpr double AGGIMAX                  = 55.0;   // PID Integrator Max (regular phase)
constexpr double STEAMKP                  = 150.0;  // PID kp (steam phase)
constexpr double AGGBKP                   = 50.0;   // PID Kp (brew detection phase)
constexpr double AGGBTN                   = 0.0;    // PID Tn (brew detection phase)
constexpr double AGGBTV                   = 20.0;   // PID Tv (brew detection phase)
constexpr double EMA_FACTOR = 0.6; // Smoothing of input that is used for Tv (derivative component of PID). Smaller
                                   // means less smoothing but also less delay, 0 means no filtering
constexpr double TARGET_BREW_TIME = 25.0; // brew time in seconds (only used if pump is being controlled)
constexpr double BREW_PID_DELAY = 10.0; // delay until enabling PID controller during brew (no heating during this time)
constexpr double PRE_INFUSION_TIME                   = 2.0;  // pre-infusion time in seconds
constexpr double PRE_INFUSION_PAUSE_TIME             = 5.0;  // pre-infusion pause time in seconds
constexpr double TARGET_BREW_WEIGHT                  = 36.0; // Target weight in grams
constexpr double STANDBY_MODE_TIME                   = 35.0; // Time in minutes until the heater is turned off
constexpr int    BACKFLUSH_CYCLES                    = 5;    // number of cycles the backflush should run
constexpr double BACKFLUSH_FILL_TIME                 = 5.0;  // time in seconds the pump is running during backflush
constexpr double BACKFLUSH_FLUSH_TIME                = 10.0; // time in seconds the 3-way valve is open during backflush
constexpr int    BACKFLUSH_REMINDER_THRESHOLD        = 50;   // shots until backflush reminder
constexpr int    BACKFLUSH_REMINDER_THRESHOLD_MIN    = 1;
constexpr int    BACKFLUSH_REMINDER_THRESHOLD_MAX    = 500;
constexpr double BACKFLUSH_REMINDER_MIN_BREW_TIME_MS = 5000.0;
constexpr float  BACKFLUSH_REMINDER_MIN_BREW_WEIGHT_G = 10.0f;
#define MAINTENANCE_STORAGE_NAMESPACE  "maintenance"
#define MAINTENANCE_SHOTS_SINCE_BF_KEY "shots_since_bf"
constexpr double POST_BREW_TIMER_DURATION   = 3.0; // time in seconds that brew timer will be shown after brew finished
constexpr int    MAXWIFIRECONNECTS          = 5;   // maximum number of reconnection attempts, use -1 to deactivate
constexpr unsigned long WIFICONNECTIONDELAY = 10000; // delay between reconnects in ms
#define MQTT_USERNAME      "rancilio"                // default MQTT username
#define MQTT_PASSWORD      "silvia"                  // default MQTT password
#define MQTT_TOPIC         "custom/kitchen/"         // default MQTT topic prefix
#define MQTT_HASSIO_PREFIX "homeassistant"           // default MQTT prefix for Home Assistant
constexpr int SCREEN_WIDTH  = 128;                   // OLED display width, in pixels
constexpr int SCREEN_HEIGHT = 64;                    // OLED display height, in pixels
#define AUTH_PASSWORD "admin"                        // default password for web authentication
#define AUTH_USERNAME "admin"                        // default username for web authentication

// Parameter range limits - converted to constexpr for type safety
constexpr double PID_KP_REGULAR_MIN            = 0.0;
constexpr double PID_KP_REGULAR_MAX            = 200.0;
constexpr double PID_TN_REGULAR_MIN            = 0.0;
constexpr double PID_TN_REGULAR_MAX            = 200.0;
constexpr double PID_TV_REGULAR_MIN            = 0.0;
constexpr double PID_TV_REGULAR_MAX            = 200.0;
constexpr double PID_I_MAX_REGULAR_MIN         = 0.0;
constexpr double PID_I_MAX_REGULAR_MAX         = 100.0;
constexpr double PID_KP_BD_MIN                 = 0.0;
constexpr double PID_KP_BD_MAX                 = 200.0;
constexpr double PID_TN_BD_MIN                 = 0.0;
constexpr double PID_TN_BD_MAX                 = 200.0;
constexpr double PID_TV_BD_MIN                 = 0.0;
constexpr double PID_TV_BD_MAX                 = 200.0;
constexpr double PID_EMA_FACTOR_MIN            = 0.0;
constexpr double PID_EMA_FACTOR_MAX            = 1.0;
constexpr double BREW_SETPOINT_MIN             = 20.0;
constexpr double BREW_SETPOINT_MAX             = 110.0;
constexpr double STEAM_SETPOINT_MIN            = 100.0;
constexpr double STEAM_SETPOINT_MAX            = 140.0;
constexpr double BREW_TEMP_OFFSET_MIN          = 0.0;
constexpr double BREW_TEMP_OFFSET_MAX          = 20.0;
constexpr double TARGET_BREW_TIME_MIN          = 1.0;
constexpr double TARGET_BREW_TIME_MAX          = 120.0;
constexpr double BREW_PID_DELAY_MIN            = 0.0;
constexpr double BREW_PID_DELAY_MAX            = 60.0;
constexpr double PRE_INFUSION_TIME_MIN         = 0.0;
constexpr double PRE_INFUSION_TIME_MAX         = 60.0;
constexpr double PRE_INFUSION_PAUSE_MIN        = 0.0;
constexpr double PRE_INFUSION_PAUSE_MAX        = 60.0;
constexpr double TARGET_BREW_WEIGHT_MIN        = 0.0;
constexpr double TARGET_BREW_WEIGHT_MAX        = 500.0;
constexpr double PID_KP_STEAM_MIN              = 0.0;
constexpr double PID_KP_STEAM_MAX              = 500.0;
constexpr double STANDBY_MODE_TIME_MIN         = 1.0;
constexpr double STANDBY_MODE_TIME_MAX         = 120.0;
constexpr int    BACKFLUSH_CYCLES_MIN          = 2;
constexpr int    BACKFLUSH_CYCLES_MAX          = 20;
constexpr double BACKFLUSH_FILL_TIME_MIN       = 3.0;
constexpr double BACKFLUSH_FILL_TIME_MAX       = 10.0;
constexpr double BACKFLUSH_FLUSH_TIME_MIN      = 5.0;
constexpr double BACKFLUSH_FLUSH_TIME_MAX      = 20.0;
constexpr double POST_BREW_TIMER_DURATION_MIN  = 0.0;
constexpr double POST_BREW_TIMER_DURATION_MAX  = 60.0;
constexpr double DISPLAY_BLINKING_DELTA        = 0.3;
constexpr double DISPLAY_BLINKING_DELTA_MIN    = 0.2;
constexpr double DISPLAY_BLINKING_DELTA_MAX    = 10.0;
constexpr int    SCALE_SAMPLES_MIN             = 1;
constexpr int    SCALE_SAMPLES_MAX             = 20;
constexpr double SCALE_CALIBRATION_MIN         = -999999.0;
constexpr double SCALE_CALIBRATION_MAX         = 999999.0;
constexpr double SCALE_KNOWN_WEIGHT_MIN        = 1.0;
constexpr double SCALE_KNOWN_WEIGHT_MAX        = 2000.0;
constexpr int    MQTT_BROKER_MAX_LENGTH        = 64;
constexpr int    USERNAME_MAX_LENGTH           = 32;
constexpr int    PASSWORD_MAX_LENGTH           = 64;
constexpr int    MQTT_TOPIC_MAX_LENGTH         = 48;
constexpr int    MQTT_HASSIO_PREFIX_MAX_LENGTH = 24;
constexpr int    HOSTNAME_MAX_LENGTH           = 64;

#ifndef VERSION
#define VERSION "4.x.x-dev"
#endif

// Conditional code removal
#ifndef FRONTEND_PREPROCESSING
#define FRONTEND_PREPROCESSING                                                                                         \
    true // enable preprocessing of frontend files (HTML, CSS, JS) to replace variables like {{hostname}} with actual
         // values
#endif

// Type-safe enum classes for hardware configuration
namespace Hardware {

// Switch types and modes (replacing integer constants)
enum class SwitchType : int {
    MOMENTARY = 0,
    TOGGLE    = 1
};

enum class SwitchMode : int {
    NORMALLY_OPEN   = 0,
    NORMALLY_CLOSED = 1
};

// Relay trigger types
enum class RelayTriggerType : int {
    LOW_TRIGGER  = 0,
    HIGH_TRIGGER = 1
};

// OLED display types
enum class OLEDType : int {
    SSD1306 = 0,
    SH1106  = 1
};

enum class OLEDAddress : int {
    ADDR_3C = 0, // 0x3C
    ADDR_3D = 1  // 0x3D
};

// Temperature sensor types
enum class TemperatureSensorType : int {
    TSIC_306       = 0,
    DALLAS_DS18B20 = 1
};

// Scale types
enum class ScaleType : int {
    HX711_DUAL   = 0, // 2 load cells
    HX711_SINGLE = 1, // 1 load cell
    BLUETOOTH    = 2
};
} // namespace Hardware

namespace System {

// Display templates
enum class DisplayTemplate : int {
    STANDARD         = 0,
    MINIMAL          = 1,
    TEMPERATURE_ONLY = 2,
    SCALE            = 3,
    UPRIGHT          = 4,
    MODERN           = 5
};

// Languages
enum class Language : int {
    ENGLISH = 0,
    GERMAN  = 1,
    SPANISH = 2
};

// Log levels (matching Logger::Level)
enum class LogLevel : int {
    TRACE   = 0,
    DEBUG   = 1,
    INFO    = 2,
    WARNING = 3,
    ERROR   = 4,
    FATAL   = 5,
    SILENT  = 6
};
} // namespace System

namespace Process {

// Brewing modes (avoiding conflict with PID library MANUAL macro)
enum class BrewMode : int {
    MANUAL_BREW    = 0,
    AUTOMATIC_BREW = 1
};
} // namespace Process

// Display constants
constexpr int DISPLAY_WIDTH     = 128;
constexpr int DISPLAY_HEIGHT    = 64;
constexpr int STATUS_BAR_HEIGHT = 12;
constexpr int STATUS_BAR_Y_POS  = 12;

// Buffer sizes
constexpr int OTA_BUFFER_SIZE     = 1024;
constexpr int MESSAGE_BUFFER_SIZE = 128;
constexpr int SHORT_MESSAGE_SIZE  = 64;

// Debugging flags
// #define DEBUG_CONFIG_VERBOSE true
// #define DEBUG_NVS_VERBOSE    true

// Conditional logging macros for different subsystems
#ifdef DEBUG_CONFIG_VERBOSE
#define LOGF_CONFIG_VERBOSE(level, format, ...) LOGF(level, format, ##__VA_ARGS__)
#define LOG_CONFIG_VERBOSE(level, message)      LOG(level, message)
#else
#define LOGF_CONFIG_VERBOSE(level, format, ...) ((void)0)
#define LOG_CONFIG_VERBOSE(level, message)      ((void)0)
#endif

#ifdef DEBUG_NVS_VERBOSE
#define LOGF_NVS_VERBOSE(level, format, ...) LOGF(level, format, ##__VA_ARGS__)
#define LOG_NVS_VERBOSE(level, message)      LOG(level, message)
#else
#define LOGF_NVS_VERBOSE(level, format, ...) ((void)0)
#define LOG_NVS_VERBOSE(level, message)      ((void)0)
#endif
