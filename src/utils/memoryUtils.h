/**
 * @file memoryUtils.h
 * @brief Memory monitoring utilities for ESP32 debugging
 */

#pragma once

#include "Logger.h"
#include <esp_heap_caps.h>
#include <esp_system.h>

/**
 * @brief Log comprehensive memory information
 * @param location Descriptive location string for logging context
 */
inline void logMemory(const char* location) {
    // Get heap information
    const size_t freeHeap = esp_get_free_heap_size();
    const size_t minFreeHeap = esp_get_minimum_free_heap_size();
    const size_t totalHeap = heap_caps_get_total_size(MALLOC_CAP_DEFAULT);
    const size_t usedHeap = totalHeap - freeHeap;
    
    // Get specific memory pool information
    const size_t freeDRAM = heap_caps_get_free_size(MALLOC_CAP_8BIT);
    const size_t totalDRAM = heap_caps_get_total_size(MALLOC_CAP_8BIT);
    const size_t usedDRAM = totalDRAM - freeDRAM;
    
    const size_t freePSRAM = heap_caps_get_free_size(MALLOC_CAP_SPIRAM);
    const size_t totalPSRAM = heap_caps_get_total_size(MALLOC_CAP_SPIRAM);
    
    // Calculate percentages
    const float heapUsagePercent = totalHeap > 0 ? (float)usedHeap * 100.0f / totalHeap : 0.0f;
    const float dramUsagePercent = totalDRAM > 0 ? (float)usedDRAM * 100.0f / totalDRAM : 0.0f;
    
    // Get largest free block
    const size_t largestFreeBlock = heap_caps_get_largest_free_block(MALLOC_CAP_DEFAULT);
    
    LOGF(INFO, "=== MEMORY @ %s ===", location);
    LOGF(INFO, "Heap: %u/%u bytes (%.1f%% used), Min: %u bytes", 
         usedHeap, totalHeap, heapUsagePercent, minFreeHeap);
    LOGF(INFO, "DRAM: %u/%u bytes (%.1f%% used)", 
         usedDRAM, totalDRAM, dramUsagePercent);
    
    if (totalPSRAM > 0) {
        const size_t usedPSRAM = totalPSRAM - freePSRAM;
        const float psramUsagePercent = (float)usedPSRAM * 100.0f / totalPSRAM;
        LOGF(INFO, "PSRAM: %u/%u bytes (%.1f%% used)", 
             usedPSRAM, totalPSRAM, psramUsagePercent);
    }
    
    LOGF(INFO, "Largest free block: %u bytes", largestFreeBlock);
    
    // Fragmentation indicator
    const float fragmentation = freeHeap > 0 ? (float)largestFreeBlock * 100.0f / freeHeap : 100.0f;
    LOGF(INFO, "Fragmentation: %.1f%% (largest/free)", fragmentation);
    
    // Memory pressure warnings
    if (freeHeap < 10000) {
        LOG(WARNING, "LOW MEMORY: Less than 10KB free heap!");
    }
    if (largestFreeBlock < 5000) {
        LOG(WARNING, "HIGH FRAGMENTATION: Largest block < 5KB!");
    }
    if (heapUsagePercent > 85.0f) {
        LOG(WARNING, "MEMORY PRESSURE: Heap usage > 85%!");
    }
    
    LOG(INFO, "=== END MEMORY ===");
}

/**
 * @brief Log basic memory information (shorter version)
 * @param location Descriptive location string for logging context
 */
inline void logMemoryBasic(const char* location) {
    const size_t freeHeap = esp_get_free_heap_size();
    const size_t largestFreeBlock = heap_caps_get_largest_free_block(MALLOC_CAP_DEFAULT);
    
    LOGF(INFO, "MEM @ %s: Free: %u, Block: %u", location, freeHeap, largestFreeBlock);
    
    if (freeHeap < 10000) {
        LOG(WARNING, "LOW MEMORY!");
    }
}

/**
 * @brief Get memory info as formatted string (for display or web interface)
 * @return String containing memory information
 */
inline String getMemoryInfo() {
    const size_t freeHeap = esp_get_free_heap_size();
    const size_t totalHeap = heap_caps_get_total_size(MALLOC_CAP_DEFAULT);
    const size_t usedHeap = totalHeap - freeHeap;
    const float heapUsagePercent = totalHeap > 0 ? (float)usedHeap * 100.0f / totalHeap : 0.0f;
    
    char buffer[128];
    snprintf(buffer, sizeof(buffer), "Heap: %u/%u (%.1f%%), Free: %u", 
             usedHeap, totalHeap, heapUsagePercent, freeHeap);
    
    return String(buffer);
}