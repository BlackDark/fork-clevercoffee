/**
 * @file BackflushModeLogic.h
 * @brief Pure helpers for backflush mode and cycle transitions (unit-tested)
 */

#pragma once

namespace CleverCoffee::Backflush {

enum class ModeChangeEffect {
    None,
    Enable,
    Disable,
    RejectedInvalidCycles,
};

struct ModeChangeInput {
    bool wasActive;
    bool requestActive;
    int  configuredCycles;
};

/**
 * @brief Resolve whether applying backflush mode should change state/flags
 */
[[nodiscard]] constexpr ModeChangeEffect resolveModeChange(const ModeChangeInput& input) noexcept {
    if (input.requestActive == input.wasActive) {
        return ModeChangeEffect::None;
    }
    if (input.requestActive && input.configuredCycles <= 0) {
        return ModeChangeEffect::RejectedInvalidCycles;
    }
    return input.requestActive ? ModeChangeEffect::Enable : ModeChangeEffect::Disable;
}

enum class CycleAdvanceEffect {
    StartNextCycle,
    CompleteAllCycles,
};

/**
 * @brief After a flush phase completes, determine if another cycle should run
 */
[[nodiscard]] constexpr CycleAdvanceEffect resolveCycleAdvance(int currentCycle, int configuredCycles) noexcept {
    if (currentCycle < configuredCycles) {
        return CycleAdvanceEffect::StartNextCycle;
    }
    return CycleAdvanceEffect::CompleteAllCycles;
}

} // namespace CleverCoffee::Backflush
