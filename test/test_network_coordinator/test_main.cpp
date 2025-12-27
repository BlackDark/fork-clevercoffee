#include <gtest/gtest.h>
#include "clevercoffee/coordinators/NetworkCoordinator.h"

using namespace CleverCoffee;

TEST(NetworkCoordinatorTest, InitiallyDisconnected) {
    NetworkCoordinator coord;
    EXPECT_FALSE(coord.isMqttConnected());
    EXPECT_FALSE(coord.isWifiConnected());
}

TEST(NetworkCoordinatorTest, CanSetMqttConnected) {
    NetworkCoordinator coord;
    coord.setMqttConnected(true);
    EXPECT_TRUE(coord.isMqttConnected());
}

TEST(NetworkCoordinatorTest, CanSetWifiConnected) {
    NetworkCoordinator coord;
    coord.setWifiConnected(true);
    EXPECT_TRUE(coord.isWifiConnected());
}

TEST(NetworkCoordinatorTest, TracksConnectionAttempts) {
    NetworkCoordinator coord;
    EXPECT_EQ(coord.getMqttConnectionAttempts(), 0);
    coord.incrementMqttConnectionAttempts();
    EXPECT_EQ(coord.getMqttConnectionAttempts(), 1);
    coord.resetMqttConnectionAttempts();
    EXPECT_EQ(coord.getMqttConnectionAttempts(), 0);
}
