#include <gtest/gtest.h>
#include "clevercoffee/coordinators/UICoordinator.h"

using namespace CleverCoffee;

TEST(UICoordinatorTest, InitiallyNotRefreshing) {
    UICoordinator coord;
    EXPECT_FALSE(coord.needsRefresh());
    EXPECT_TRUE(coord.shouldAutoSleep());
}

TEST(UICoordinatorTest, CanRequestRefresh) {
    UICoordinator coord;
    coord.requestRefresh();
    EXPECT_TRUE(coord.needsRefresh());
    coord.clearRefresh();
    EXPECT_FALSE(coord.needsRefresh());
}

TEST(UICoordinatorTest, CanDisableAutoSleep) {
    UICoordinator coord;
    coord.setAutoSleep(false);
    EXPECT_FALSE(coord.shouldAutoSleep());
}
