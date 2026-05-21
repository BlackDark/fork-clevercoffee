/**
 * @file test_main.cpp
 * @brief Unit tests for backflush reminder / maintenance coordinator
 */

#include <gtest/gtest.h>

#include "../test_support.h"
#include "clevercoffee/maintenance/BackflushReminderLogic.h"
#include "clevercoffee/coordinators/MaintenanceCoordinator.h"
#include "clevercoffee/Config.h"
#include "clevercoffee/defaults.h"

#include "../mocks/ConfigStubs.cpp"
#include "../../src/Logger.cpp"
#include "../../src/coordinators/MaintenanceCoordinator.cpp"

using CleverCoffee::MaintenanceCoordinator;
using CleverCoffee::Maintenance::qualifiesAsCountedShot;

class BackflushReminderLogicTest : public ::testing::Test {};

TEST_F(BackflushReminderLogicTest, RejectsShortBrewWithoutScaleWeight) {
    EXPECT_FALSE(qualifiesAsCountedShot(3000.0, 0.0f, false));
}

TEST_F(BackflushReminderLogicTest, AcceptsMinimumDuration) {
    EXPECT_TRUE(qualifiesAsCountedShot(BACKFLUSH_REMINDER_MIN_BREW_TIME_MS, 0.0f, false));
}

TEST_F(BackflushReminderLogicTest, AcceptsWeightWhenScaleEnabled) {
    EXPECT_TRUE(qualifiesAsCountedShot(1000.0, BACKFLUSH_REMINDER_MIN_BREW_WEIGHT_G, true));
}

TEST_F(BackflushReminderLogicTest, RejectsShortBrewWithLowWeightWhenScaleEnabled) {
    EXPECT_FALSE(qualifiesAsCountedShot(1000.0, 5.0f, true));
}

TEST_F(BackflushReminderLogicTest, ReminderDueBoundary) {
    EXPECT_FALSE(MaintenanceCoordinator::isReminderDueForCount(49, true, 50));
    EXPECT_TRUE(MaintenanceCoordinator::isReminderDueForCount(50, true, 50));
    EXPECT_FALSE(MaintenanceCoordinator::isReminderDueForCount(50, false, 50));
}

class MaintenanceCoordinatorTest : public ::testing::Test {
protected:
    void SetUp() override {
        Preferences::resetTestStore();
        coordinator_ = std::make_unique<MaintenanceCoordinator>();
        ASSERT_TRUE(coordinator_->begin());
    }

    void TearDown() override {
        coordinator_.reset();
        Preferences::resetTestStore();
    }

    std::unique_ptr<MaintenanceCoordinator> coordinator_;
};

TEST_F(MaintenanceCoordinatorTest, CountsQualifiedBrewAndPersists) {
    coordinator_->recordBrewIfQualified(BACKFLUSH_REMINDER_MIN_BREW_TIME_MS, 0.0f, false);
    EXPECT_EQ(1, coordinator_->getShotsSinceBackflush());

    auto reloaded = std::make_unique<MaintenanceCoordinator>();
    ASSERT_TRUE(reloaded->begin());
    EXPECT_EQ(1, reloaded->getShotsSinceBackflush());
}

TEST_F(MaintenanceCoordinatorTest, SkipsUnqualifiedBrew) {
    coordinator_->recordBrewIfQualified(1000.0, 0.0f, false);
    EXPECT_EQ(0, coordinator_->getShotsSinceBackflush());
}

TEST_F(MaintenanceCoordinatorTest, ResetClearsCounter) {
    coordinator_->recordBrewIfQualified(BACKFLUSH_REMINDER_MIN_BREW_TIME_MS, 0.0f, false);
    coordinator_->resetSinceBackflush();
    EXPECT_EQ(0, coordinator_->getShotsSinceBackflush());

    auto reloaded = std::make_unique<MaintenanceCoordinator>();
    ASSERT_TRUE(reloaded->begin());
    EXPECT_EQ(0, reloaded->getShotsSinceBackflush());
}

TEST_F(MaintenanceCoordinatorTest, ReminderDueUsesConfigDefaults) {
    for (int i = 0; i < BACKFLUSH_REMINDER_THRESHOLD; ++i) {
        coordinator_->recordBrewIfQualified(BACKFLUSH_REMINDER_MIN_BREW_TIME_MS, 0.0f, false);
    }
    EXPECT_TRUE(coordinator_->isReminderDue());
    EXPECT_TRUE(coordinator_->consumeReminderAnnouncement());
    EXPECT_FALSE(coordinator_->consumeReminderAnnouncement());
}

TEST_F(MaintenanceCoordinatorTest, DisabledReminderStillCountsButNotDue) {
    Config::getInstance().maintenanceBackflushReminderEnabled.set(false);

    for (int i = 0; i < BACKFLUSH_REMINDER_THRESHOLD; ++i) {
        coordinator_->recordBrewIfQualified(BACKFLUSH_REMINDER_MIN_BREW_TIME_MS, 0.0f, false);
    }

    EXPECT_EQ(BACKFLUSH_REMINDER_THRESHOLD, coordinator_->getShotsSinceBackflush());
    EXPECT_FALSE(coordinator_->isReminderDue());
    EXPECT_FALSE(coordinator_->consumeReminderAnnouncement());

    Config::getInstance().maintenanceBackflushReminderEnabled.set(true);
    coordinator_->onReminderConfigChanged();
    EXPECT_TRUE(coordinator_->isReminderDue());
    EXPECT_TRUE(coordinator_->consumeReminderAnnouncement());
}
