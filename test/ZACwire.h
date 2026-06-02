/**
 * @file ZACwire.h
 * @brief Controllable ZACwire stub for native test environment
 */

#pragma once

#include <deque>

// Controllable ZACwire stub. Tests queue readings via setNext(); getTemp() pops
// the next queued value (or returns the steady value() if the queue is empty).
class ZACwire {
public:
    ZACwire(int /*pin*/, int /*sensorType*/ = 306) {}
    bool begin() { return true; }
    float getTemp(int /*maxChangeRate*/ = 1) {
        if (!queue().empty()) {
            const float v = queue().front();
            queue().pop_front();
            return v;
        }
        return value();
    }
    bool available() { return true; }

    // --- Test controls ---
    static float& value() {
        static float v = 0.0f;
        return v;
    }
    static std::deque<float>& queue() {
        static std::deque<float> q;
        return q;
    }
    static void reset() {
        value() = 0.0f;
        queue().clear();
    }
    static void setNext(float v) { queue().push_back(v); }
};
