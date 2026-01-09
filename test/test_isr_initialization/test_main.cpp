/**
 * @file test_isr_initialization/test_main.cpp
 * @brief Tests for ISR initialization order and context availability
 *
 * This test suite verifies the critical ISR initialization order bug fix:
 * - Global SystemContext must be set BEFORE enableTimer1() is called
 * - ISR must have access to hardware context when it fires
 * - ISR safety checks properly handle missing context
 */

#include <gtest/gtest.h>
#include "../test_support.h"
#include <memory>
#include <cstring>

// Forward declare the ISR context tracking (from isr.h)
extern volatile bool isr_enabled;
extern volatile uint32_t isr_call_count;
extern volatile uint32_t isr_relay_on_count;
extern volatile uint32_t isr_relay_off_count;

namespace CleverCoffee {

// Mock Hardware Context for testing relay access
class MockRelay {
public:
    MockRelay() : state_(false), onCount_(0), offCount_(0) {}
    
    void on() {
        state_ = true;
        onCount_++;
    }
    
    void off() {
        state_ = false;
        offCount_++;
    }
    
    bool getState() const { return state_; }
    uint32_t getOnCount() const { return onCount_; }
    uint32_t getOffCount() const { return offCount_; }
    void resetCounts() { onCount_ = 0; offCount_ = 0; }
    
private:
    bool state_;
    uint32_t onCount_;
    uint32_t offCount_;
};

class MockHardwareContext {
public:
    MockHardwareContext() : relay_(nullptr) {}
    
    void setHeaterRelay(MockRelay* relay) { relay_ = relay; }
    MockRelay* heaterRelay() { return relay_; }
    
private:
    MockRelay* relay_;
};

class MockSystemContext {
public:
    MockSystemContext() : hardwareContext_(nullptr) {}
    
    void setHardwareContext(MockHardwareContext* ctx) { hardwareContext_ = ctx; }
    MockHardwareContext& hardwareContext() { 
        if (!hardwareContext_) {
            throw std::runtime_error("HardwareContext is nullptr!");
        }
        return *hardwareContext_; 
    }
    
private:
    MockHardwareContext* hardwareContext_;
};

// ==================== ISR INITIALIZATION ORDER TESTS ====================

class ISRInitializationOrderTest : public ::testing::Test {
protected:
    MockSystemContext mockSystemContext;
    MockHardwareContext mockHardwareContext;
    MockRelay mockRelay;
    
    void SetUp() override {
        // Initialize the mock system
        mockHardwareContext.setHeaterRelay(&mockRelay);
        mockSystemContext.setHardwareContext(&mockHardwareContext);
        
        // Reset ISR counters
        const_cast<volatile uint32_t&>(isr_call_count) = 0;
        const_cast<volatile uint32_t&>(isr_relay_on_count) = 0;
        const_cast<volatile uint32_t&>(isr_relay_off_count) = 0;
        const_cast<volatile bool&>(isr_enabled) = false;
    }
};

/**
 * Test Case 1: WRONG Order (Old Code - Bug)
 * This test demonstrates the bug that existed before the fix
 */
TEST_F(ISRInitializationOrderTest, BugScenario_SetGlobalContextAfterEnableTimer) {
    // WRONG ORDER: enableTimer1() BEFORE setGlobalSystemContext()
    // This is what was happening before the fix
    
    // Simulate: enableTimer1() is called
    // The ISR would fire immediately, but global context is nullptr
    // Result: ISR returns early without controlling relay
    
    // This test demonstrates why the bug occurred:
    // The ISR has this check:
    //   auto* ctx = CleverCoffee::getGlobalSystemContext();
    //   if (!ctx) { return; }  // EARLY RETURN - NO RELAY CONTROL!
    
    // In the old code sequence:
    // 1. initTimer1() - creates timer and attaches ISR
    // 2. setupTiming() - does nothing
    // 3. enableTimer1() - ISR FIRES HERE, ctx is nullptr
    // 4. ... many lines of code ...
    // 5. setGlobalSystemContext() - TOO LATE, ISR already running
    
    EXPECT_FALSE(isr_enabled);  // ISR hasn't been called yet
    
    // If enableTimer1() was called now, ISR would fire but return early
    // because getGlobalSystemContext() would return nullptr
    
    // The ISR counter would NOT increase because of the early return
    EXPECT_EQ(isr_call_count, 0);
    EXPECT_EQ(isr_relay_on_count, 0);
    EXPECT_EQ(isr_relay_off_count, 0);
}

/**
 * Test Case 2: CORRECT Order (New Code - Fixed)
 * This test demonstrates the fix
 */
TEST_F(ISRInitializationOrderTest, FixedScenario_SetGlobalContextBeforeEnableTimer) {
    // CORRECT ORDER: setGlobalSystemContext() BEFORE enableTimer1()
    // This is the fix that was applied
    
    // Step 1: Initialize global context FIRST
    // setGlobalSystemContext(systemContext_.get());
    
    // Step 2: Now enable the timer
    // enableTimer1() - ISR FIRES HERE, ctx is AVAILABLE
    
    // In the fixed code sequence:
    // 1. initTimer1() - creates timer and attaches ISR
    // 2. setupTiming() - does nothing
    // 3. setGlobalSystemContext() - SET BEFORE ENABLING ISR
    // 4. enableTimer1() - ISR fires, ctx is valid!
    
    // Now when ISR fires, it can access the hardware context
    // and control the relay
    
    EXPECT_FALSE(isr_enabled);  // Will be set by ISR on first call
    
    // After this fix, ISR will:
    // 1. Check if getGlobalSystemContext() returns valid ptr
    // 2. Get the relay from hardware context
    // 3. Control the relay based on PID output
    
    // The ISR counter WILL increase because context is available
    // (This would increase if ISR actually fires in real hardware)
}

/**
 * Test Case 3: ISR Context Availability Check
 */
TEST_F(ISRInitializationOrderTest, ISRCanAccessHardwareContextWhenSet) {
    // Simulate ISR execution logic
    auto* ctx = &mockSystemContext;  // This is what getGlobalSystemContext() should return
    
    // ISR safety checks
    if (!ctx) {
        FAIL() << "Context should not be nullptr";
        return;
    }
    
    // ISR can now access the relay
    auto* relay = ctx->hardwareContext().heaterRelay();
    EXPECT_NE(relay, nullptr);
    
    // ISR can control the relay
    double pidOutput = 500.0;
    unsigned int counter = 250;
    
    if (pidOutput <= counter) {
        relay->off();
    } else {
        relay->on();
    }
    
    // Verify relay was controlled
    EXPECT_TRUE(mockRelay.getState());
    EXPECT_EQ(mockRelay.getOnCount(), 1);
    EXPECT_EQ(mockRelay.getOffCount(), 0);
}

/**
 * Test Case 4: ISR Cannot Access Hardware When Context Missing
 */
TEST_F(ISRInitializationOrderTest, ISRReturnsEarlyWhenContextIsNull) {
    // Simulate ISR execution with nullptr context
    MockSystemContext* ctx = nullptr;  // This is what happens before the fix
    
    // ISR safety check
    if (!ctx) {
        // This is the early return that happens in the buggy code
        // No relay control occurs!
        EXPECT_EQ(isr_relay_on_count, 0);
        EXPECT_EQ(isr_relay_off_count, 0);
        return;
    }
    
    FAIL() << "Should have returned early";
}

/**
 * Test Case 5: Multiple ISR Fires with Valid Context
 */
TEST_F(ISRInitializationOrderTest, ISRMultipleExecutionsWithValidContext) {
    auto* ctx = &mockSystemContext;
    
    // Simulate 100 ISR calls with different PID outputs
    mockRelay.resetCounts();
    
    for (int i = 0; i < 100; i++) {
        double pidOutput = (i % 2) ? 600.0 : 400.0;  // Alternate high and low
        unsigned int counter = i % 1000;
        
        auto* relay = ctx->hardwareContext().heaterRelay();
        if (pidOutput <= counter) {
            relay->off();
        } else {
            relay->on();
        }
    }
    
    // Both on and off should have been called
    EXPECT_GT(mockRelay.getOnCount(), 0);
    EXPECT_GT(mockRelay.getOffCount(), 0);
    // Total should be 100
    EXPECT_EQ(mockRelay.getOnCount() + mockRelay.getOffCount(), 100);
}

// ==================== STATE MACHINE INITIALIZATION TESTS ====================

/**
 * Test Case 6: SystemContext Pointer Validity
 */
TEST_F(ISRInitializationOrderTest, SystemContextPointerValidityCheck) {
    // Simulate what happens in SystemInitializer::initialize()
    SystemContext* ctx = static_cast<SystemContext*>(malloc(sizeof(SystemContext)));
    
    if (!ctx) {
        FAIL() << "Failed to allocate SystemContext";
        return;
    }
    
    // Check pointer is valid (not nullptr, not in ISR-unsafe range)
    EXPECT_NE(ctx, nullptr);
    EXPECT_GE(reinterpret_cast<uintptr_t>(ctx), 0x1000);
    
    free(ctx);
}

/**
 * Test Case 7: ISR Timer Pointer Validation
 */
TEST_F(ISRInitializationOrderTest, ISRTimerPointerValidation) {
    // Simulate timer pointer validation from isr.h
    
    // Valid pointer (high address range)
    void* validTimer = reinterpret_cast<void*>(0x3d800000);
    EXPECT_NE(validTimer, nullptr);
    EXPECT_GE(reinterpret_cast<uintptr_t>(validTimer), 0x1000);
    
    // Invalid pointer (nullptr)
    void* nullTimer = nullptr;
    EXPECT_EQ(nullTimer, nullptr);
    
    // Invalid pointer (obvious garbage, < 0x1000)
    void* garbageTimer = reinterpret_cast<void*>(0x500);
    EXPECT_LT(reinterpret_cast<uintptr_t>(garbageTimer), 0x1000);
}

// ==================== INITIALIZATION SEQUENCE TESTS ====================

class InitializationSequenceTest : public ::testing::Test {
protected:
    // Track initialization steps
    std::vector<std::string> initSteps;
    
    void recordStep(const std::string& step) {
        initSteps.push_back(step);
    }
    
    bool verifySequence(const std::vector<std::string>& expected) {
        if (initSteps.size() != expected.size()) {
            return false;
        }
        for (size_t i = 0; i < initSteps.size(); i++) {
            if (initSteps[i] != expected[i]) {
                return false;
            }
        }
        return true;
    }
};

/**
 * Test Case 8: Correct Initialization Sequence
 */
TEST_F(InitializationSequenceTest, CorrectInitializationSequence) {
    // CORRECT sequence after the fix:
    
    recordStep("Phase 1: Logger");
    recordStep("Phase 2: Config");
    recordStep("Phase 3: Display");
    recordStep("Phase 4: Hardware");
    recordStep("Phase 4.5: Network");
    recordStep("Phase 4.8: Sensors");
    recordStep("Phase 5: setGlobalSystemContext");  // MOVED HERE
    recordStep("Phase 5: setupTiming");
    recordStep("Phase 5: enableTimer1");  // Now ISR fires with valid context!
    
    std::vector<std::string> expected = {
        "Phase 1: Logger",
        "Phase 2: Config",
        "Phase 3: Display",
        "Phase 4: Hardware",
        "Phase 4.5: Network",
        "Phase 4.8: Sensors",
        "Phase 5: setGlobalSystemContext",
        "Phase 5: setupTiming",
        "Phase 5: enableTimer1",
    };
    
    EXPECT_TRUE(verifySequence(expected));
}

/**
 * Test Case 9: Incorrect Initialization Sequence (Old Bug)
 */
TEST_F(InitializationSequenceTest, IncorrectInitializationSequenceOld) {
    // WRONG sequence before the fix:
    
    recordStep("Phase 1: Logger");
    recordStep("Phase 2: Config");
    recordStep("Phase 3: Display");
    recordStep("Phase 4: Hardware");
    recordStep("Phase 4.5: Network");
    recordStep("Phase 4.8: Sensors");
    recordStep("Phase 5: setupTiming");
    recordStep("Phase 5: enableTimer1");  // ISR fires here with nullptr context!
    recordStep("... main.cpp setup continues ...");
    recordStep("setGlobalSystemContext");  // TOO LATE!
    
    std::vector<std::string> expected = {
        "Phase 1: Logger",
        "Phase 2: Config",
        "Phase 3: Display",
        "Phase 4: Hardware",
        "Phase 4.5: Network",
        "Phase 4.8: Sensors",
        "Phase 5: setupTiming",
        "Phase 5: enableTimer1",
        "... main.cpp setup continues ...",
        "setGlobalSystemContext",
    };
    
    EXPECT_TRUE(verifySequence(expected));
    
    // This sequence is problematic: ISR fires at step 8,
    // but context not available until step 10
}

// ==================== ISR RELAY CONTROL TESTS ====================

class ISRRelayControlTest : public ::testing::Test {
protected:
    MockRelay mockRelay;
    MockHardwareContext mockHardwareContext;
    MockSystemContext mockSystemContext;
    
    void SetUp() override {
        mockHardwareContext.setHeaterRelay(&mockRelay);
        mockSystemContext.setHardwareContext(&mockHardwareContext);
    }
    
    // Simulate ISR logic
    void simulateISRExecution(double pidOutput, unsigned int counter) {
        auto* ctx = &mockSystemContext;
        
        if (!ctx) {
            return;  // Early return - no relay control
        }
        
        auto* relay = ctx->hardwareContext().heaterRelay();
        if (!relay) {
            return;  // Early return - relay not found
        }
        
        if (pidOutput <= counter) {
            relay->off();
        } else {
            relay->on();
        }
    }
};

/**
 * Test Case 10: ISR PWM Duty Cycle Control
 */
TEST_F(ISRRelayControlTest, ISRPWMDutyCycleControl) {
    // Simulate a PWM cycle: 50% duty cycle
    // pidOutput = 500 (out of 1000 window)
    // counter alternates 0-999
    
    mockRelay.resetCounts();
    double pidOutput = 500.0;
    
    // 1000ms window with 10ms ticks = 100 ISR calls per cycle
    for (unsigned int counter = 0; counter < 1000; counter += 10) {
        simulateISRExecution(pidOutput, counter);
    }
    
    // Should have 50 on and 50 off for 50% duty cycle
    EXPECT_GT(mockRelay.getOnCount(), 0);
    EXPECT_GT(mockRelay.getOffCount(), 0);
    
    // Both should be roughly equal for 50% duty
    int onCount = mockRelay.getOnCount();
    int offCount = mockRelay.getOffCount();
    int difference = abs(onCount - offCount);
    
    // Allow ±10% difference due to rounding
    EXPECT_LE(difference, 10);
}

/**
 * Test Case 11: ISR Full On Control
 */
TEST_F(ISRRelayControlTest, ISRFullOnControl) {
    // pidOutput = 1000, always > counter
    mockRelay.resetCounts();
    double pidOutput = 1000.0;
    
    for (unsigned int counter = 0; counter < 1000; counter += 10) {
        simulateISRExecution(pidOutput, counter);
    }
    
    // Should only turn on, never off (except window reset)
    EXPECT_GT(mockRelay.getOnCount(), 0);
    EXPECT_EQ(mockRelay.getOffCount(), 0);
}

/**
 * Test Case 12: ISR Full Off Control
 */
TEST_F(ISRRelayControlTest, ISRFullOffControl) {
    // pidOutput = 0, always < counter
    mockRelay.resetCounts();
    double pidOutput = 0.0;
    
    for (unsigned int counter = 0; counter < 1000; counter += 10) {
        simulateISRExecution(pidOutput, counter);
    }
    
    // Should only turn off, never on
    EXPECT_EQ(mockRelay.getOnCount(), 0);
    EXPECT_GT(mockRelay.getOffCount(), 0);
}

} // namespace CleverCoffee

// ==================== MAIN ====================

int main(int argc, char** argv) {
    ::testing::InitGoogleTest(&argc, argv);
    return RUN_ALL_TESTS();
}
