#!/bin/bash

echo "🧪 Running Comprehensive CleverCoffee Parameter System Tests"
echo "============================================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

total_tests=0
passed_tests=0

run_test() {
    local test_name="$1"
    local test_file="$2"
    local output_file="$3"

    echo -e "\n${BLUE}Running: $test_name${NC}"
    echo "----------------------------------------"

    total_tests=$((total_tests + 1))

    # Compile
    echo "Compiling $test_file..."
    if g++ -std=c++17 -O2 -Wall -Wextra "$test_file" -o "$output_file" 2>&1; then
        echo "✓ Compilation successful"

        # Run
        echo "Executing test..."
        if "./$output_file"; then
            echo -e "${GREEN}✅ $test_name PASSED${NC}"
            passed_tests=$((passed_tests + 1))
            return 0
        else
            echo -e "${RED}❌ $test_name FAILED (runtime error)${NC}"
            return 1
        fi
    else
        echo -e "${RED}❌ $test_name FAILED (compilation error)${NC}"
        return 1
    fi
}

# Run all tests
echo -e "${YELLOW}Starting test suite...${NC}\n"

run_test "Comprehensive Parameter System Tests" \
         "test/test_comprehensive_parameter_system.cpp" \
         "test/parameter_system_test"

run_test "Enum Static Mappings Tests" \
         "test/test_enum_static_mappings.cpp" \
         "test/enum_mappings_test"

run_test "Web API Compatibility Tests" \
         "test/test_web_api_compatibility.cpp" \
         "test/web_api_test"

run_test "Enum Type Values Tests" \
         "test/test_enum_types.cpp" \
         "test/test_enum_types"

# Summary
echo ""
echo "============================================================"
if [ $passed_tests -eq $total_tests ]; then
    echo -e "${GREEN}🎉 ALL TESTS PASSED! ($passed_tests/$total_tests)${NC}"
    echo -e "${GREEN}✅ Parameter system is working correctly${NC}"
    echo ""
    echo "Test Coverage:"
    echo "• ✅ Basic parameter operations (set/get)"
    echo "• ✅ Parameter validation (ranges, types)"
    echo "• ✅ NVS persistence (save/load)"
    echo "• ✅ Enum options with static mappings"
    echo "• ✅ Global variable binding"
    echo "• ✅ Error handling"
    echo "• ✅ Enum ordering stability"
    echo "• ✅ Web API JSON serialization"
    echo "• ✅ Frontend compatibility"
    echo ""
    echo "The parameter system is ready for production! 🚀"
    exit 0
else
    echo -e "${RED}❌ SOME TESTS FAILED ($passed_tests/$total_tests passed)${NC}"
    echo -e "${RED}Please fix the failing tests before proceeding${NC}"
    exit 1
fi
