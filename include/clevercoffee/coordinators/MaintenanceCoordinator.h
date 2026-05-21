/**
 * @file MaintenanceCoordinator.h
 * @brief Tracks shots since last backflush and backflush reminder state
 */

#pragma once

#include "clevercoffee/maintenance/BackflushReminderLogic.h"

namespace CleverCoffee {

class MaintenanceCoordinator {
  public:
    MaintenanceCoordinator() = default;

    [[nodiscard]] bool begin();

    void recordBrewIfQualified(double totalBrewTimeMs, float brewWeight, bool scaleEnabled);

    void resetSinceBackflush();

    void onReminderConfigChanged();

    [[nodiscard]] int getShotsSinceBackflush() const noexcept {
        return shotsSinceBackflush_;
    }

    [[nodiscard]] bool isReminderDue() const;

    [[nodiscard]] static bool isReminderDueForCount(int shots, bool enabled, int threshold) noexcept {
        return Maintenance::isReminderDueForCount(shots, enabled, threshold);
    }

    /**
     * @brief Returns true once when reminder becomes due (boot or threshold cross).
     */
    [[nodiscard]] bool consumeReminderAnnouncement();

  private:
    void persistShotsSinceBackflush() const;

    int  shotsSinceBackflush_{0};
    bool announcementPending_{false};
};

} // namespace CleverCoffee
