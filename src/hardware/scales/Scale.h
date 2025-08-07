
/**
 * @file Scale.h
 * @brief Scale interface and implementations
 */

#pragma once

#if __cplusplus >= 202002L
#include <concepts>

// C++20 concept for weight values
template<typename T>
concept WeightValue = std::floating_point<T> && requires(T t) {
    { t >= -1000.0f } -> std::convertible_to<bool>; // Reasonable minimum (tare offset)
    { t <= 10000.0f } -> std::convertible_to<bool>; // Reasonable maximum in grams
};
#endif

/**
 * @brief Abstract base class for scale implementations
 */
class Scale {
    public:
        virtual ~Scale() = default;

        /**
         * @brief Initialize the scale
         * @return true if initialization successful, false otherwise
         */
        virtual bool init() = 0;

        /**
         * @brief Check if scale data is available and update readings
         * @return true if new data is available, false otherwise
         */
        virtual bool update() = 0;

        /**
         * @brief Get the current weight reading
         * @return Weight in grams
         */
        [[nodiscard]] virtual float getWeight() const noexcept = 0;

        /**
         * @brief Tare the scale (set current weight as zero point)
         */
        virtual void tare() = 0;

        /**
         * @brief Set the number of samples to use for readings
         * @param samples Number of samples
         */
        virtual void setSamples(int samples) = 0;

        /**
         * @brief Check if scale is connected (for Bluetooth scales)
         * @return true if connected, false for wired scales or if not connected
         */
        [[nodiscard]] virtual bool isConnected() const noexcept {
            return true;
        }

#if __cplusplus >= 202002L
        /**
         * @brief Validate weight reading with concepts
         * @param weight Weight value to validate
         * @return true if weight is within valid range
         */
        template<WeightValue T>
        static constexpr bool isValidWeight(T weight) noexcept {
            return weight >= -500.0f && weight <= 5000.0f; // Practical range for coffee scales
        }
#else
        /**
         * @brief Validate weight reading (C++17 fallback)
         * @param weight Weight value to validate
         * @return true if weight is within valid range
         */
        static constexpr bool isValidWeight(float weight) noexcept {
            return weight >= -500.0f && weight <= 5000.0f; // Practical range for coffee scales
        }
#endif
};
