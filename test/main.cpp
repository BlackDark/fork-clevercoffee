/**
 * @file main.cpp
 * @brief GoogleTest and GMock main entry point for PlatformIO native tests
 *
 * This file provides the main() function for all GoogleTest and GMock tests.
 * All test files should only contain TEST(), TEST_F(), or mock definitions,
 * NOT their own main() function.
 *
 * GMock is initialized here to enable mock object verification and expectations.
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>

// Initialize and run all GoogleTest and GMock tests
int main(int argc, char** argv) {
    ::testing::InitGoogleTest(&argc, argv);
    // GMock initialization - required for mock object verification
    ::testing::InitGoogleMock(&argc, argv);
    
    return RUN_ALL_TESTS();
}
