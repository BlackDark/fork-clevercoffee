/**
 * @file esp_task_wdt.h
 * @brief ESP32 Task Watchdog Timer stub for native tests
 *
 * Provides stub implementations of ESP32 watchdog timer functions
 * for native testing environments.
 */

#pragma once

#include <cstdint>

// ESP32 error codes
typedef int esp_err_t;
#define ESP_OK 0
#define ESP_FAIL -1
#define ESP_ERR_INVALID_STATE 0x103

/**
 * @brief Initialize the Task Watchdog Timer
 * @param timeout_sec Timeout in seconds
 * @param panic If true, panic on timeout
 * @return ESP_OK always for stubs
 */
inline esp_err_t esp_task_wdt_init(uint32_t timeout_sec, bool panic) {
    (void)timeout_sec;
    (void)panic;
    return ESP_OK;
}

/**
 * @brief Subscribe a task to the watchdog timer
 * @param task_handle Task handle (nullptr = current task)
 * @return ESP_OK always for stubs
 */
inline esp_err_t esp_task_wdt_add(void* task_handle) {
    (void)task_handle;
    return ESP_OK;
}

/**
 * @brief Reset the watchdog timer (feed the dog)
 * @return ESP_OK always for stubs
 */
inline esp_err_t esp_task_wdt_reset() {
    return ESP_OK;
}

/**
 * @brief Unsubscribe a task from the watchdog timer
 * @param task_handle Task handle (nullptr = current task)
 * @return ESP_OK always for stubs
 */
inline esp_err_t esp_task_wdt_delete(void* task_handle) {
    (void)task_handle;
    return ESP_OK;
}

/**
 * @brief Deinitialize the Task Watchdog Timer
 * @return ESP_OK always for stubs
 */
inline esp_err_t esp_task_wdt_deinit() {
    return ESP_OK;
}


/**
 * @brief Subscribe the Arduino loop task to the watchdog timer
 *
 * Provided by esp32-hal-misc.c on hardware; stubbed here because native tests
 * compile with -DESP32.
 */
inline void enableLoopWDT() {}

/**
 * @brief Unsubscribe the Arduino loop task from the watchdog timer
 */
inline void disableLoopWDT() {}
