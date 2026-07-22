/**
 * @file test_main.cpp
 * @brief Hostname must be applied before STA begin for DHCP client ID
 */

#include "../test_support.h"
#include "network/WiFiStaConnect.h"

#include <WiFi.h>
#include <gtest/gtest.h>

class WiFiStaHostnameTest : public ::testing::Test {
  protected:
    void SetUp() override {
        WiFi.resetTestState();
    }
};

TEST_F(WiFiStaHostnameTest, BeginWithPasswordSetsHostnameBeforeBegin) {
    CleverCoffee::Network::beginStaWithHostname("my-silvia", "HomeNet", "secret");

    EXPECT_EQ(WiFi.testSetHostnameCallCount(), 1);
    EXPECT_EQ(WiFi.testHostname(), String("my-silvia"));
    EXPECT_TRUE(WiFi.testHostnameSetBeforeBegin());
    EXPECT_EQ(WiFi.testBeginCallCount(), 1);
    EXPECT_EQ(WiFi.testLastSsid(), String("HomeNet"));
    EXPECT_EQ(WiFi.testLastPassword(), String("secret"));
}

TEST_F(WiFiStaHostnameTest, BeginOpenNetworkSetsHostnameBeforeBegin) {
    CleverCoffee::Network::beginStaWithHostname("open-box", "OpenSSID", "");

    EXPECT_TRUE(WiFi.testHostnameSetBeforeBegin());
    EXPECT_EQ(WiFi.testHostname(), String("open-box"));
    EXPECT_EQ(WiFi.testLastSsid(), String("OpenSSID"));
    EXPECT_TRUE(WiFi.testLastPassword().isEmpty());
}

TEST_F(WiFiStaHostnameTest, ReconnectAppliesHostnameBeforeBegin) {
    CleverCoffee::Network::reconnectStaWithHostname("rejoin-host");

    EXPECT_EQ(WiFi.testSetHostnameCallCount(), 1);
    EXPECT_EQ(WiFi.testHostname(), String("rejoin-host"));
    EXPECT_TRUE(WiFi.testHostnameSetBeforeBegin());
    EXPECT_EQ(WiFi.testBeginCallCount(), 1);
    EXPECT_EQ(WiFi.status(), WiFiClass::WL_CONNECTED);
}

TEST_F(WiFiStaHostnameTest, EmptyHostnameStillBegins) {
    CleverCoffee::Network::beginStaWithHostname("", "Net", "pw");

    EXPECT_EQ(WiFi.testSetHostnameCallCount(), 0);
    EXPECT_EQ(WiFi.testBeginCallCount(), 1);
    EXPECT_EQ(WiFi.testLastSsid(), String("Net"));
}
