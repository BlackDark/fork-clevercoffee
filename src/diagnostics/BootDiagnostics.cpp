/**
 * @file BootDiagnostics.cpp
 * @brief ESP32 capture of reset reason and core-dump summary
 */

#include "clevercoffee/diagnostics/BootDiagnostics.h"

#include "clevercoffee/Logger.h"

#include <cstdio>
#include <cstdlib>
#include <esp_system.h>
#include <memory>

#if CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH && CONFIG_ESP_COREDUMP_DATA_FORMAT_ELF
#include <esp_core_dump.h>
#endif

void CleverCoffee::BootDiagnostics::capture() {
    resetReason_ = static_cast<int>(esp_reset_reason());
    LOGF(INFO, "Reset reason: %s", resetReasonToString(resetReason_));

    crashInfo_[0] = '\0';

#if CONFIG_ESP_COREDUMP_ENABLE_TO_FLASH && CONFIG_ESP_COREDUMP_DATA_FORMAT_ELF
    if (esp_core_dump_image_check() != ESP_OK) {
        return;
    }

    std::unique_ptr<esp_core_dump_summary_t, void (*)(void*)> summary(
        static_cast<esp_core_dump_summary_t*>(std::malloc(sizeof(esp_core_dump_summary_t))), std::free);
    if (!summary) {
        LOG(WARNING, "Not enough memory to read the core dump summary");
        return;
    }

    if (esp_core_dump_get_summary(summary.get()) == ESP_OK) {
        std::snprintf(crashInfo_,
                      kCrashInfoSize,
                      "task=%s cause=%u pc=0x%08x vaddr=0x%08x",
                      summary->exc_task,
                      static_cast<unsigned>(summary->ex_info.exc_cause),
                      static_cast<unsigned>(summary->exc_pc),
                      static_cast<unsigned>(summary->ex_info.exc_vaddr));
        LOGF(INFO, "Core dump found: %s", crashInfo_);
    } else {
        LOG(WARNING, "Core dump present but the summary could not be read");
    }
#endif
}
