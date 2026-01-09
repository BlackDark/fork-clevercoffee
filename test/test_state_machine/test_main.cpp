/**
 * @file test_main.cpp
 * @brief Tests for StateMachine reference_wrapper usage
 */

#include <gtest/gtest.h>
#include "../test_support.h"
#include <functional>

#include "clevercoffee/state/MachineState.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/MachineStateContext.h"

using namespace CleverCoffee;

// ==================== REFERENCE_WRAPPER SEMANTICS TESTS ====================

class ReferenceWrapperSemanticsTest : public ::testing::Test {
protected:
    // Minimal mock state for testing
    class MockState : public MachineState {
    public:
        explicit MockState(MachineStateId id) : id_(id), updateCount(0), entryCount(0), exitCount(0) {}

        void update(MachineStateContext& context) override {
            updateCount++;
        }

        MachineState* checkTransitions(MachineStateContext& context) override {
            return nullptr;  // No transitions by default
        }

        MachineStateId getStateId() const override {
            return id_;
        }

        const char* getStateName() const override {
            return "MockState";
        }

        // Test helpers
        int getUpdateCount() const { return updateCount; }
        int getEntryCount() const { return entryCount; }
        int getExitCount() const { return exitCount; }

    private:
        MachineStateId id_;
        int updateCount;
        int entryCount;
        int exitCount;
    };

    void SetUp() override {
        // Create test states
        state1 = std::make_unique<MockState>(MachineStateId::PID_NORMAL);
        state2 = std::make_unique<MockState>(MachineStateId::BREW_IDLE);
    }

    std::unique_ptr<MockState> state1;
    std::unique_ptr<MockState> state2;
};

TEST_F(ReferenceWrapperSemanticsTest, ReferenceWrapperCannotBeNull) {
    // reference_wrapper wraps a reference, cannot be null
    std::reference_wrapper<MachineState> ref(*state1);

    // Can access the wrapped object
    EXPECT_EQ(ref.get().getStateId(), MachineStateId::PID_NORMAL);
    EXPECT_NE(ref.get().getStateId(), MachineStateId::BREW_IDLE);
}

TEST_F(ReferenceWrapperSemanticsTest, ReferenceWrapperPreservesObjectIdentity) {
    std::reference_wrapper<MachineState> ref(*state1);

    // The address should match the original object
    EXPECT_EQ(&ref.get(), state1.get());
    EXPECT_EQ(&ref.get(), &(*state1));
}

TEST_F(ReferenceWrapperSemanticsTest, ReferenceWrapperCanBeReassigned) {
    std::reference_wrapper<MachineState> ref(*state1);

    // Initially wraps state1
    EXPECT_EQ(ref.get().getStateId(), MachineStateId::PID_NORMAL);

    // Can be reassigned to state2
    ref = *state2;
    EXPECT_EQ(ref.get().getStateId(), MachineStateId::BREW_IDLE);
}

TEST_F(ReferenceWrapperSemanticsTest, ReferenceWrapperAllowsPointerComparison) {
    std::reference_wrapper<MachineState> ref1(*state1);
    std::reference_wrapper<MachineState> ref2(*state1);
    std::reference_wrapper<MachineState> ref3(*state2);

    // Can compare addresses
    EXPECT_EQ(&ref1.get(), &ref2.get());
    EXPECT_NE(&ref1.get(), &ref3.get());
}

TEST_F(ReferenceWrapperSemanticsTest, ReferenceWrapperIsCopyable) {
    std::reference_wrapper<MachineState> ref1(*state1);
    std::reference_wrapper<MachineState> ref2 = ref1;  // Copy

    // Both reference the same object
    EXPECT_EQ(&ref1.get(), &ref2.get());
    EXPECT_EQ(&ref1.get(), state1.get());
}

// ==================== STATE TRANSITION SIMULATION TESTS ====================

class StateTransitionSimulationTest : public ::testing::Test {
protected:
    class MockState : public MachineState {
    public:
        explicit MockState(MachineStateId id) : id_(id), entryCallCount(0), exitCallCount(0) {}

        void onEntry(MachineStateContext& context) override {
            entryCallCount++;
        }

        void onExit(MachineStateContext& context) override {
            exitCallCount++;
        }

        void update(MachineStateContext& context) override {}

        MachineState* checkTransitions(MachineStateContext& context) override {
            return nullptr;
        }

        MachineStateId getStateId() const override { return id_; }
        const char* getStateName() const override { return "MockState"; }

        int getEntryCallCount() const { return entryCallCount; }
        int getExitCallCount() const { return exitCallCount; }

    private:
        MachineStateId id_;
        int entryCallCount;
        int exitCallCount;
    };

    void SetUp() override {
        state1 = std::make_unique<MockState>(MachineStateId::PID_NORMAL);
        state2 = std::make_unique<MockState>(MachineStateId::BREW_IDLE);
    }

    std::unique_ptr<MockState> state1;
    std::unique_ptr<MockState> state2;
};

TEST_F(StateTransitionSimulationTest, SimulateStateTransitionWithReferenceWrapper) {
    std::reference_wrapper<MachineState> currentState(*state1);

    // Initially in state1
    EXPECT_EQ(currentState.get().getStateId(), MachineStateId::PID_NORMAL);

    // Can reassign to state2
    currentState = *state2;

    // Now in state2
    EXPECT_EQ(currentState.get().getStateId(), MachineStateId::BREW_IDLE);
}

TEST_F(StateTransitionSimulationTest, SimulateMultipleTransitions) {
    std::reference_wrapper<MachineState> currentState(*state1);

    // Track transitions
    MachineStateId expectedStates[] = {
        MachineStateId::PID_NORMAL,
        MachineStateId::BREW_IDLE,
        MachineStateId::PID_NORMAL,
        MachineStateId::BREW_IDLE
    };

    for (int i = 0; i < 4; i++) {
        EXPECT_EQ(currentState.get().getStateId(), expectedStates[i]);

        // Transition to next state
        currentState = (i % 2 == 0) ? *state2 : *state1;
    }
}

TEST_F(StateTransitionSimulationTest, PreventSelfTransition) {
    std::reference_wrapper<MachineState> currentState(*state1);

    // Get the address of current state
    MachineState* currentStatePtr = &currentState.get();

    // Verify we can detect self-transitions by address comparison
    EXPECT_EQ(&currentState.get(), state1.get());
    EXPECT_NE(&currentState.get(), state2.get());
}

// ==================== LIFETIME AND OWNERSHIP TESTS ====================

class LifetimeAndOwnershipTest : public ::testing::Test {
protected:
    class MockState : public MachineState {
    public:
        explicit MockState(MachineStateId id) : id_(id), alive(true) {}

        ~MockState() {
            alive = false;
        }

        void update(MachineStateContext& context) override {}

        MachineState* checkTransitions(MachineStateContext& context) override {
            return nullptr;
        }

        MachineStateId getStateId() const override { return id_; }
        const char* getStateName() const override { return "MockState"; }

        bool isAlive() const { return alive; }

    private:
        MachineStateId id_;
        bool alive;
    };
};

TEST_F(LifetimeAndOwnershipTest, ReferenceWrapperDoesNotOwnObject) {
    auto state = std::make_unique<MockState>(MachineStateId::PID_NORMAL);
    std::reference_wrapper<MachineState> ref(*state);

    // reference_wrapper does not extend lifetime
    EXPECT_TRUE(static_cast<MockState&>(ref.get()).isAlive());

    // Destroy the object
    state.reset();

    // reference_wrapper now references a destroyed object
    // This demonstrates that reference_wrapper is non-owning
    // (In real code, this would be undefined behavior)
}

TEST_F(LifetimeAndOwnershipTest, ReferenceWrapperWithSingletonPattern) {
    // Simulates how StateMachine will use reference_wrapper
    // with singleton states from StateFactory

    static MockState singletonState(MachineStateId::PID_NORMAL);
    std::reference_wrapper<MachineState> ref(singletonState);

    // Singleton has static storage duration, always valid
    EXPECT_EQ(ref.get().getStateId(), MachineStateId::PID_NORMAL);
    EXPECT_TRUE(singletonState.isAlive());

    // Safe to use throughout program lifetime
}

// ==================== COMPILATION TESTS ====================

TEST(CompilationTest, ReferenceWrapperCompilesWithMachineState) {
    // This test verifies that reference_wrapper<MachineState> compiles
    // It's a minimal compilation test

    class MinimalState : public MachineState {
    public:
        MinimalState() : id_(MachineStateId::PID_NORMAL) {}

        void update(MachineStateContext& context) override {}
        MachineState* checkTransitions(MachineStateContext& context) override { return nullptr; }
        MachineStateId getStateId() const override { return id_; }
        const char* getStateName() const override { return "MinimalState"; }

    private:
        MachineStateId id_;
    };

    MinimalState state;
    std::reference_wrapper<MachineState> ref(state);

    // If this compiles and runs, the test passes
    EXPECT_EQ(ref.get().getStateId(), MachineStateId::PID_NORMAL);
}

TEST(CompilationTest, ReferenceWrapperCanBeStoredAsMember) {
    // Verifies that reference_wrapper can be used as a class member
    // (which is how StateMachine will use it)

    class Container {
    public:
        explicit Container(MachineState& state) : state_(state) {}

        MachineState& getState() { return state_.get(); }
        const MachineState& getState() const { return state_.get(); }

    private:
        std::reference_wrapper<MachineState> state_;
    };

    class MinimalState : public MachineState {
    public:
        MinimalState() : id_(MachineStateId::PID_NORMAL) {}

        void update(MachineStateContext& context) override {}
        MachineState* checkTransitions(MachineStateContext& context) override { return nullptr; }
        MachineStateId getStateId() const override { return id_; }
        const char* getStateName() const override { return "MinimalState"; }

    private:
        MachineStateId id_;
    };

    MinimalState state;
    Container container(state);

    EXPECT_EQ(container.getState().getStateId(), MachineStateId::PID_NORMAL);
}
