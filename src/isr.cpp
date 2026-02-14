/**
 * @file isr.cpp
 * @brief Implementation of ISR-specific SystemContext accessor
 */

#include "clevercoffee/isr.h"

#include "clevercoffee/context/SystemContext.h"

// Atomic counters for ISR debugging - single definitions
std::atomic<bool>     isr_enabled{false};
std::atomic<uint32_t> isr_call_count{0};
std::atomic<uint32_t> isr_relay_on_count{0};
std::atomic<uint32_t> isr_relay_off_count{0};

namespace CleverCoffee {
namespace ISR {
// Single static pointer for ISR use only
static SystemContext* isr_context = nullptr;

void setSystemContext(SystemContext* context) noexcept {
    if (!isr_context && context) {
        isr_context = context;
    }
}

SystemContext* getSystemContext() noexcept {
    return isr_context;
}
} // namespace ISR
} // namespace CleverCoffee
