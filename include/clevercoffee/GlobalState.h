/**
 * @file GlobalState.h
 * @brief Forwarding header for backward compatibility
 *
 * This file has been deprecated. All symbols are now defined in GlobalTypes.h.
 * It remains as a forwarding header for backward compatibility with existing code.
 *
 * NEW CODE SHOULD INCLUDE:
 * - include/clevercoffee/types/GlobalTypes.h (for types and constants)
 *
 * MIGRATION NOTES:
 * - The global g_state variable has been completely eliminated
 * - State is now managed through SystemContext dependency injection
 * - All dead code structs have been removed
 * - This header is provided only for build compatibility
 */

#pragma once

// Forward to the new types header
#include "clevercoffee/types/GlobalTypes.h"
