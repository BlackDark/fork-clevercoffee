/**
 * @file ConfigStub.cpp
 * @brief Stub implementation of Config for tests
 * 
 * Provides minimal implementations of Config functions needed for tests.
 * Note: Most Config functionality is header-only (ParamDef), but we need
 * to provide the singleton instance.
 */

#include "clevercoffee/Config.h"
#include "../test_support.h"

// Config singleton is already defined in header as:
// static Config& getInstance() { static Config instance; return instance; }
// So we don't need to implement it here - it's header-only.

// If Config has any .cpp implementations that need stubbing, add them here.
// For now, Config is mostly header-only with ParamDef members.
