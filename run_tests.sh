#!/bin/bash

# Script to compile and run parameter system tests

echo "Compiling parameter system tests..."

# Compile the test
g++ -std=c++17 -O2 -Wall -Wextra \
    test/test_comprehensive_parameter_system.cpp \
    -o test/parameter_system_test

if [ $? -eq 0 ]; then
    echo "✓ Compilation successful"
    echo ""
    echo "Running tests..."
    echo "===================="
    ./test/parameter_system_test
    test_result=$?
    echo "===================="

    if [ $test_result -eq 0 ]; then
        echo "✅ All tests passed!"
    else
        echo "❌ Tests failed!"
        exit 1
    fi
else
    echo "❌ Compilation failed!"
    exit 1
fi
