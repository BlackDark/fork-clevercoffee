/* Arduino.h for the display parity oracle.
 *
 * U8g2lib.h does `#include <Arduino.h>` and `#include <Print.h>` under
 * `#ifdef ARDUINO` (U8g2lib.h:48-51), and the firmware's own headers expect
 * `millis()`, `String`, `constrain`, `map` and the `PROGMEM` family. This
 * forwards to `ArduinoShim.h`, which holds the actual definitions.
 *
 * Never compiled into the firmware.
 */

#pragma once

#include "ArduinoShim.h"
