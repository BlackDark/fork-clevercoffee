/**
 * @file PubSubClient.h
 * @brief PubSubClient stub for native test environment
 */

#pragma once

#include <String.h>

// Minimal PubSubClient stub
class PubSubClient {
public:
    PubSubClient() = default;
    virtual ~PubSubClient() = default;
    
    bool connect(const char* id) { return true; }
    bool connect(const char* id, const char* user, const char* pass) { return true; }
    bool publish(const char* topic, const char* payload) { return true; }
    bool subscribe(const char* topic) { return true; }
    bool loop() { return true; }
    bool connected() { return true; }
    void disconnect() {}
};
