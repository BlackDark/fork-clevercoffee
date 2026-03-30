/**
 * @file esp_system.h
 * @brief ESP32 system functions stub for native tests
 */

#pragma once

#include <cstddef>

inline size_t esp_get_free_heap_size() {
    return 1000000;  // Return 1MB free
}

inline size_t esp_get_minimum_free_heap_size() {
    return 100000;  // Return 100KB minimum free
}

inline size_t esp_get_heap_size() {
    return 2000000;  // Return 2MB total
}
