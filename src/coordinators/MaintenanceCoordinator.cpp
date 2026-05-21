/**
 * @file MaintenanceCoordinator.cpp
 * @brief Implementation of maintenance / backflush reminder coordination
 */

#include "clevercoffee/coordinators/MaintenanceCoordinator.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/defaults.h"

#include <Preferences.h>

namespace CleverCoffee {

bool MaintenanceCoordinator::begin() {
    Preferences prefs;
    if (!prefs.begin(MAINTENANCE_STORAGE_NAMESPACE, true)) {
        LOG(WARNING, "Maintenance: failed to open NVS namespace for reading");
        return false;
    }

    shotsSinceBackflush_ = prefs.getInt(MAINTENANCE_SHOTS_SINCE_BF_KEY, 0);
    prefs.end();

    LOGF(INFO, "Maintenance: loaded %d shots since backflush", shotsSinceBackflush_);
    return true;
}

void MaintenanceCoordinator::recordBrewIfQualified(double totalBrewTimeMs, float brewWeight, bool scaleEnabled) {
    if (!Maintenance::qualifiesAsCountedShot(totalBrewTimeMs, brewWeight, scaleEnabled)) {
        LOGF(DEBUG,
             "Maintenance: brew not counted (time=%.0fms, weight=%.1fg, scale=%s)",
             totalBrewTimeMs,
             brewWeight,
             scaleEnabled ? "on" : "off");
        return;
    }

    const int previousCount = shotsSinceBackflush_;
    ++shotsSinceBackflush_;

    if (!persistShotsSinceBackflush()) {
        shotsSinceBackflush_ = previousCount;
        LOG(ERROR, "Maintenance: reverted shot count after NVS write failure");
        return;
    }

    LOGF(INFO, "Maintenance: counted brew, shots since backflush = %d", shotsSinceBackflush_);
}

void MaintenanceCoordinator::resetSinceBackflush() {
    if (shotsSinceBackflush_ == 0) {
        return;
    }

    const int previousCount = shotsSinceBackflush_;
    shotsSinceBackflush_    = 0;

    if (!persistShotsSinceBackflush()) {
        shotsSinceBackflush_ = previousCount;
        LOG(ERROR, "Maintenance: reverted reset after NVS write failure");
        return;
    }

    LOG(INFO, "Maintenance: reset shots since backflush");
}

bool MaintenanceCoordinator::isReminderDue() const {
    return isReminderDueForCount(shotsSinceBackflush_,
                                 Config::getInstance().maintenanceBackflushReminderEnabled.get(),
                                 Config::getInstance().maintenanceBackflushReminderThreshold.get());
}

bool MaintenanceCoordinator::persistShotsSinceBackflush() const {
    Preferences prefs;
    if (!prefs.begin(MAINTENANCE_STORAGE_NAMESPACE, false)) {
        LOG(ERROR, "Maintenance: failed to open NVS namespace for writing");
        return false;
    }

    const bool ok = prefs.putInt(MAINTENANCE_SHOTS_SINCE_BF_KEY, shotsSinceBackflush_) > 0;
    prefs.end();

    if (!ok) {
        LOG(ERROR, "Maintenance: failed to persist shots since backflush");
    }

    return ok;
}

} // namespace CleverCoffee
