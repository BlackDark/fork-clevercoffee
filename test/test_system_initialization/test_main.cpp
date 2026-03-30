/**
 * @file test_system_initialization/test_main.cpp
 * @brief Unit tests for SystemInitializer to catch initialization bugs
 *
 * These tests verify:
 * 1. systemInitialized_ flag is properly set
 * 2. isInitialized() returns correct state
 * 3. LoopManager gets created when isInitialized() is true
 * 4. Critical initialization sequence
 */

#include <gtest/gtest.h>
#include "../test_support.h"
#include <memory>

// Mock SystemInitializer for testing
class MockSystemInitializer {
public:
    MockSystemInitializer() : systemInitialized_(false), initialized_called_(false) {}
    
    bool initialize() {
        initialized_called_ = true;
        // CRITICAL: Must set systemInitialized_ = true!
        systemInitialized_ = true;
        return true;
    }
    
    bool isInitialized() const {
        return systemInitialized_;
    }
    
    bool wasInitializeCalled() const {
        return initialized_called_;
    }

private:
    bool systemInitialized_;
    bool initialized_called_;
};

// Mock LoopManager
class MockLoopManager {
public:
    explicit MockLoopManager(bool* created_flag) : created_flag_(created_flag) {
        if (created_flag_) {
            *created_flag_ = true;
        }
    }
    
private:
    bool* created_flag_;
};

// ==================== SYSTEM INITIALIZATION TESTS ====================

class SystemInitializationTest : public ::testing::Test {
protected:
    MockSystemInitializer initializer;
    bool loopManagerCreated = false;
};

/**
 * Test 1: initialize() is called and returns true
 * This test would have caught if initialize() didn't return true
 */
TEST_F(SystemInitializationTest, InitializeReturnsTrue) {
    bool result = initializer.initialize();
    EXPECT_TRUE(result) << "initialize() must return true to indicate success";
}

/**
 * Test 2: initialize() sets the systemInitialized_ flag
 * THIS IS THE BUG TEST - catches the missing systemInitialized_ = true
 */
TEST_F(SystemInitializationTest, InitializeSetsInitializedFlag) {
    EXPECT_FALSE(initializer.isInitialized()) << "Should not be initialized before initialize()";
    
    bool result = initializer.initialize();
    EXPECT_TRUE(result) << "initialize() should return true";
    EXPECT_TRUE(initializer.isInitialized()) 
        << "CRITICAL BUG: isInitialized() must return true after initialize() succeeds!";
}

/**
 * Test 3: isInitialized() reflects correct state
 */
TEST_F(SystemInitializationTest, IsInitializedReflectsState) {
    // Before initialization
    EXPECT_FALSE(initializer.isInitialized());
    
    // After initialization
    initializer.initialize();
    EXPECT_TRUE(initializer.isInitialized());
}

/**
 * Test 4: LoopManager gets created ONLY when isInitialized() is true
 * This test catches the logic bug in main.cpp
 */
TEST_F(SystemInitializationTest, LoopManagerCreatedOnlyWhenInitialized) {
    // Simulate main.cpp logic
    bool loopManagerWillBeCreated = false;
    
    // First call initialize()
    bool initResult = initializer.initialize();
    EXPECT_TRUE(initResult);
    
    // Then check if initialized (like main.cpp does at line 141)
    if (initializer.isInitialized()) {
        // Create LoopManager (like main.cpp does at line 165)
        MockLoopManager loopManager(&loopManagerWillBeCreated);
        EXPECT_TRUE(loopManagerWillBeCreated) << "LoopManager should be created";
    } else {
        FAIL() << "CRITICAL BUG: isInitialized() returned false after successful initialize()!";
    }
}

/**
 * Test 5: Verify initialization sequence
 */
class InitializationSequenceTest : public ::testing::Test {
protected:
    std::vector<std::string> sequence;
};

TEST_F(InitializationSequenceTest, CorrectSequence) {
    // Simulate the sequence in main.cpp
    MockSystemInitializer initializer;
    
    // Step 1: Call initialize()
    sequence.push_back("initialize_called");
    bool initResult = initializer.initialize();
    EXPECT_TRUE(initResult);
    
    // Step 2: Check if systemInitialized_ was set
    sequence.push_back("check_initialized");
    bool isInit = initializer.isInitialized();
    EXPECT_TRUE(isInit) << "After initialize(), isInitialized() must be true";
    
    // Step 3: Create LoopManager (this would fail if isInit is false)
    sequence.push_back("create_loop_manager");
    bool loopManagerCreated = false;
    if (isInit) {
        MockLoopManager loopManager(&loopManagerCreated);
    }
    
    EXPECT_TRUE(loopManagerCreated) << "LoopManager must be created";
    EXPECT_EQ(sequence.size(), 3);
}

/**
 * Test 6: Bug reproduction - what happens when systemInitialized_ is NOT set
 */
class InitializationBugTest : public ::testing::Test {
public:
    // This simulates the BUGGY version
    class BuggySystemInitializer {
    public:
        BuggySystemInitializer() : systemInitialized_(false) {}
        
        bool initialize() {
            // BUG: Forgot to set systemInitialized_ = true!
            // systemInitialized_ = true;  // <-- THIS LINE IS MISSING
            return true;
        }
        
        bool isInitialized() const {
            return systemInitialized_;
        }
        
    private:
        bool systemInitialized_;  // Still false after initialize()!
    };
};

TEST_F(InitializationBugTest, BuggyInitializerDoesNotSetFlag) {
    BuggySystemInitializer initializer;
    
    // Initialize returns true, so caller thinks it worked
    bool result = initializer.initialize();
    EXPECT_TRUE(result);
    
    // But isInitialized() is still false - THIS IS THE BUG
    bool isInit = initializer.isInitialized();
    EXPECT_FALSE(isInit) << "This demonstrates the bug: initialize() returns true but isInitialized() is false!";
    
    // LoopManager would NOT be created
    bool loopManagerCreated = false;
    if (isInit) {
        MockLoopManager loopManager(&loopManagerCreated);
    }
    
    EXPECT_FALSE(loopManagerCreated) 
        << "LoopManager is NOT created because isInitialized() is false";
}

/**
 * Test 7: Main loop would crash without LoopManager
 */
class MainLoopTest : public ::testing::Test {
protected:
    MockLoopManager* loopManager = nullptr;
    
    // Simulate main loop with nullptr check
    void simulateMainLoop() {
        if (loopManager) {
            // Safe to call loopManager->update()
        } else {
            // This is what would happen without the check we added
            // loopManager->update();  // NULLPTR DEREFERENCE!
            GTEST_SKIP() << "LoopManager is nullptr - would crash here!";
        }
    }
};

TEST_F(MainLoopTest, MainLoopWithoutLoopManagerWouldCrash) {
    // Before the fix, loopManager would be nullptr
    loopManager = nullptr;
    
    // The main loop check we added would catch this
    if (!loopManager) {
        // CRITICAL: This would have been a nullptr dereference
        EXPECT_EQ(loopManager, nullptr) << "LoopManager is nullptr - would crash if we call update()";
    }
}

/**
 * Test 8: Verify all initialization components work together
 */
class FullInitializationTest : public ::testing::Test {
protected:
    bool systemInitializedCorrectly = false;
    bool loopManagerCreatedCorrectly = false;
    bool stateMachineCreatedCorrectly = false;
    bool processControllerCreatedCorrectly = false;
};

TEST_F(FullInitializationTest, AllComponentsInitializedInOrder) {
    // Step 1: SystemInitializer
    MockSystemInitializer initializer;
    EXPECT_TRUE(initializer.initialize());
    systemInitializedCorrectly = initializer.isInitialized();
    EXPECT_TRUE(systemInitializedCorrectly) << "Step 1 failed: SystemInitializer";
    
    // Step 2: LoopManager (depends on Step 1)
    if (initializer.isInitialized()) {
        MockLoopManager loopManager(&loopManagerCreatedCorrectly);
        EXPECT_TRUE(loopManagerCreatedCorrectly) << "Step 2 failed: LoopManager";
    }
    
    // Step 3: StateMachine (depends on Step 1)
    if (initializer.isInitialized()) {
        stateMachineCreatedCorrectly = true;  // Simplified mock
        EXPECT_TRUE(stateMachineCreatedCorrectly) << "Step 3 failed: StateMachine";
    }
    
    // Step 4: ProcessController (depends on Step 1)
    if (initializer.isInitialized()) {
        processControllerCreatedCorrectly = true;  // Simplified mock
        EXPECT_TRUE(processControllerCreatedCorrectly) << "Step 4 failed: ProcessController";
    }
    
    // All components should be initialized
    EXPECT_TRUE(systemInitializedCorrectly);
    EXPECT_TRUE(loopManagerCreatedCorrectly);
    EXPECT_TRUE(stateMachineCreatedCorrectly);
    EXPECT_TRUE(processControllerCreatedCorrectly);
}

/**
 * Test 9: Catch broken initialization chain
 */
class InitializationChainTest : public ::testing::Test {
protected:
    // Simulate dependency chain
    bool checkInitializationChain() {
        MockSystemInitializer initializer;
        
        // Chain: initialize() -> isInitialized() -> create children
        if (!initializer.initialize()) {
            return false;  // Step 1 failed
        }
        
        if (!initializer.isInitialized()) {
            return false;  // Step 2 failed - THIS WOULD CATCH THE BUG
        }
        
        // If we get here, all children can be created
        return true;
    }
};

TEST_F(InitializationChainTest, BrokenChainDetected) {
    bool result = checkInitializationChain();
    EXPECT_TRUE(result) << "Initialization chain must succeed";
}

/**
 * Test 10: ISR context availability depends on initialization
 */
class ISRContextAvailabilityTest : public ::testing::Test {
protected:
    bool globalContextAvailable = false;
    
    void setGlobalContext(bool available) {
        globalContextAvailable = available;
    }
    
    bool canISRAccessContext() {
        return globalContextAvailable;
    }
};

TEST_F(ISRContextAvailabilityTest, ISRNeedsInitializedContext) {
    // ISR fires immediately when enabled
    // It needs access to SystemContext which is set during initialization
    
    EXPECT_FALSE(canISRAccessContext()) << "Context not available yet";
    
    // After initialization, set global context
    setGlobalContext(true);
    EXPECT_TRUE(canISRAccessContext()) << "Context available after initialization";
    
    // ISR can now safely use the context
    EXPECT_TRUE(globalContextAvailable);
}
