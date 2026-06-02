/**
 * @file MockOledDriver.h
 * @brief Mock implementation of OledDriver for testing
 */

#pragma once

#include <gmock/gmock.h>

class MockOledDriver {
  public:
    MockOledDriver()          = default;
    virtual ~MockOledDriver() = default;

    MOCK_METHOD(bool, initialize, (), ());
    MOCK_METHOD(void, prepareDisplay, (), ());
    MOCK_METHOD(void, forceUpdate, (), ());
    MOCK_METHOD(bool, isBufferReady, (), (const));
    MOCK_METHOD(void, setBufferReady, (bool), ());
    MOCK_METHOD(bool, isUpdateRunning, (), (const));
    MOCK_METHOD(void, setUpdateRunning, (bool), ());
};
