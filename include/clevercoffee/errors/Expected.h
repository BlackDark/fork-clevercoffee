#pragma once

#include <variant>
#include <string>

namespace CleverCoffee {

/**
 * @brief Simple Expected type for error handling
 *
 * Since ESP32 doesn't have std::expected yet, this provides
 * a similar interface using std::variant.
 */
template<typename T, typename E = std::string>
class Expected {
public:
    Expected(T value) : data_(std::move(value)) {}
    Expected(E error) : data_(std::move(error)) {}

    bool hasValue() const noexcept {
        return std::holds_alternative<T>(data_);
    }

    const T& value() const& {
        return std::get<T>(data_);
    }

    T& value() & {
        return std::get<T>(data_);
    }

    const E& error() const& {
        return std::get<E>(data_);
    }

    explicit operator bool() const noexcept {
        return hasValue();
    }

private:
    std::variant<T, E> data_;
};

} // namespace CleverCoffee
