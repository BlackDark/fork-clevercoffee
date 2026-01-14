/**
 * @file MockUIManager.h
 * @brief Mock implementation of UIManager for testing
 *
 * Allows testing code that depends on UIManager without actual display operations.
 */

#pragma once

#include <gmock/gmock.h>
#include <String.h>

/**
 * @class MockUIManager
 * @brief Mock implementation of UIManager for testing
 *
 * Provides EXPECT_CALL() support for UI operations.
 * 
 * Example usage:
 * @code
 * MockUIManager mockUI;
 * EXPECT_CALL(mockUI, initialize()).WillOnce(Return(true));
 * EXPECT_CALL(mockUI, update()).Times(AtLeast(1));
 * @endcode
 */
class MockUIManager {
public:
    MockUIManager() = default;
    virtual ~MockUIManager() = default;

    /**
     * @brief Mock method to initialize UI manager
     */
    MOCK_METHOD(bool, initialize, (), ());

    /**
     * @brief Mock method to update UI
     */
    MOCK_METHOD(void, update, (), ());

    /**
     * @brief Mock method to prepare display
     */
    MOCK_METHOD(void, prepareDisplay, (), ());

    /**
     * @brief Mock method to display logo
     */
    MOCK_METHOD(void, displayLogo, (const String&, const String&), ());

    /**
     * @brief Mock method to display message
     */
    MOCK_METHOD(void, displayMessage, (const String&, const String&, const String&, 
                                       const String&, const String&, const String&), ());

    /**
     * @brief Mock method to check if brew timer should be displayed
     */
    MOCK_METHOD(bool, shouldDisplayBrewTimer, (), ());

    /**
     * @brief Mock method to display brew time
     */
    MOCK_METHOD(void, displayBrewTime, (), ());

    /**
     * @brief Mock method to display fullscreen brew timer
     */
    MOCK_METHOD(void, displayFullscreenBrewTimer, (), ());

    /**
     * @brief Mock method to check if buffer is ready
     */
    MOCK_METHOD(bool, isBufferReady, (), (const));

    /**
     * @brief Mock method to set buffer ready state
     */
    MOCK_METHOD(void, setBufferReady, (bool), ());

    /**
     * @brief Mock method to check if update is running
     */
    MOCK_METHOD(bool, isUpdateRunning, (), (const));

    /**
     * @brief Mock method to set update running state
     */
    MOCK_METHOD(void, setUpdateRunning, (bool), ());

    /**
     * @brief Mock method to force display update
     */
    MOCK_METHOD(void, forceUpdate, (), ());
};
