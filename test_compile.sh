#!/bin/bash

echo "Testing WebServerManager compilation..."

cd /Users/marbaced/tmp/clevercoffee-claude

# Try to compile just the WebServerManager object file
pio run -e esp32_usb --target .pio/build/esp32_usb/src/network/WebServerManager.cpp.o

echo "WebServerManager compilation test complete."
