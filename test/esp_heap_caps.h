/**
 * @file esp_heap_caps.h
 * @brief ESP32 heap capabilities stub for native tests
 */

#pragma once

// Stub for ESP32 heap capabilities - not needed in native tests
#define MALLOC_CAP_DEFAULT 0
#define MALLOC_CAP_INTERNAL 1
#define MALLOC_CAP_SPIRAM 2
#define MALLOC_CAP_8BIT 3

inline size_t heap_caps_get_free_size(uint32_t caps) {
    (void)caps;
    return 1000000;  // Return 1MB free
}

inline size_t heap_caps_get_largest_free_block(uint32_t caps) {
    (void)caps;
    return 500000;  // Return 500KB largest block
}

inline size_t heap_caps_get_total_size(uint32_t caps) {
    (void)caps;
    return 2000000;  // Return 2MB total
}

inline size_t heap_caps_get_minimum_free_size(uint32_t caps) {
    (void)caps;
    return 100000;  // Return 100KB minimum free
}
