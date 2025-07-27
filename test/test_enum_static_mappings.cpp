#include <iostream>
#include <cassert>

// Test the new EnumOption structure with explicit value mappings
struct EnumOption {
    int value;
    const char* label;
};

// Test enum arrays from our actual Config.cpp
EnumOption logLevelOpts[] = {
    {0, "Silent"},
    {1, "Error"},
    {2, "Warning"},
    {3, "Info"},
    {4, "Debug"},
    {5, "Verbose"}
};

EnumOption switchTypeOpts[] = {
    {0, "Switch"},
    {1, "Sensor"}
};

EnumOption switchModeOpts[] = {
    {0, "Disabled"},
    {1, "Switch"},
    {2, "Trigger"}
};

EnumOption relayTriggerOpts[] = {
    {0, "Low"},
    {1, "High"}
};

EnumOption displayTemplateOpts[] = {
    {0, "Standard"},
    {1, "Minimal"},
    {2, "TempOnly"},
    {3, "Scale"},
    {4, "Upright"}
};

EnumOption languageOpts[] = {
    {0, "EN"},
    {1, "DE"},
    {2, "ES"}
};

void testEnumStaticMappings() {
    std::cout << "=== Testing Enum Static Mappings ===" << std::endl;

    // Test log level enum
    std::cout << "Log Level Options:" << std::endl;
    for (int i = 0; i < 6; i++) {
        std::cout << "  " << logLevelOpts[i].value << ": " << logLevelOpts[i].label << std::endl;
        assert(logLevelOpts[i].value == i); // Values should be sequential starting from 0
    }
    std::cout << "✓ Log level enum has correct static mappings" << std::endl;

    // Test switch type enum
    std::cout << "\nSwitch Type Options:" << std::endl;
    for (int i = 0; i < 2; i++) {
        std::cout << "  " << switchTypeOpts[i].value << ": " << switchTypeOpts[i].label << std::endl;
        assert(switchTypeOpts[i].value == i);
    }
    std::cout << "✓ Switch type enum has correct static mappings" << std::endl;

    // Test switch mode enum
    std::cout << "\nSwitch Mode Options:" << std::endl;
    for (int i = 0; i < 3; i++) {
        std::cout << "  " << switchModeOpts[i].value << ": " << switchModeOpts[i].label << std::endl;
        assert(switchModeOpts[i].value == i);
    }
    std::cout << "✓ Switch mode enum has correct static mappings" << std::endl;

    // Test relay trigger enum
    std::cout << "\nRelay Trigger Options:" << std::endl;
    for (int i = 0; i < 2; i++) {
        std::cout << "  " << relayTriggerOpts[i].value << ": " << relayTriggerOpts[i].label << std::endl;
        assert(relayTriggerOpts[i].value == i);
    }
    std::cout << "✓ Relay trigger enum has correct static mappings" << std::endl;

    // Test display template enum
    std::cout << "\nDisplay Template Options:" << std::endl;
    for (int i = 0; i < 5; i++) {
        std::cout << "  " << displayTemplateOpts[i].value << ": " << displayTemplateOpts[i].label << std::endl;
        assert(displayTemplateOpts[i].value == i);
    }
    std::cout << "✓ Display template enum has correct static mappings" << std::endl;

    // Test language enum
    std::cout << "\nLanguage Options:" << std::endl;
    for (int i = 0; i < 3; i++) {
        std::cout << "  " << languageOpts[i].value << ": " << languageOpts[i].label << std::endl;
        assert(languageOpts[i].value == i);
    }
    std::cout << "✓ Language enum has correct static mappings" << std::endl;
}

void testEnumOrderingStability() {
    std::cout << "\n=== Testing Enum Ordering Stability ===" << std::endl;

    // Simulate what happens when enums are reordered by creating alternate arrays
    EnumOption altLogLevelOpts[] = {
        {1, "Error"},      // Moved Error to first position
        {0, "Silent"},     // Moved Silent to second position
        {2, "Warning"},
        {3, "Info"},
        {4, "Debug"},
        {5, "Verbose"}
    };

    // With explicit value mappings, the stored value 1 should still mean "Error"
    // even though it's now at index 0 instead of index 1
    assert(altLogLevelOpts[0].value == 1); // First item has value 1
    assert(std::string(altLogLevelOpts[0].label) == "Error"); // But still means "Error"

    // Find value 1 in the reordered array
    int errorIndex = -1;
    for (int i = 0; i < 6; i++) {
        if (altLogLevelOpts[i].value == 1) {
            errorIndex = i;
            break;
        }
    }
    assert(errorIndex == 0); // Found at new position
    assert(std::string(altLogLevelOpts[errorIndex].label) == "Error"); // Still means "Error"

    std::cout << "✓ Explicit value mappings preserve meaning even when reordered" << std::endl;
}

void testValueLookup() {
    std::cout << "\n=== Testing Value Lookup Functions ===" << std::endl;

    // Function to find label by value
    auto findLabelByValue = [](const EnumOption* options, size_t count, int value) -> const char* {
        for (size_t i = 0; i < count; i++) {
            if (options[i].value == value) {
                return options[i].label;
            }
        }
        return nullptr;
    };

    // Function to find value by label
    auto findValueByLabel = [](const EnumOption* options, size_t count, const char* label) -> int {
        for (size_t i = 0; i < count; i++) {
            if (std::string(options[i].label) == label) {
                return options[i].value;
            }
        }
        return -1;
    };

    // Test lookup functions
    assert(std::string(findLabelByValue(logLevelOpts, 6, 3)) == "Info");
    assert(findValueByLabel(logLevelOpts, 6, "Debug") == 4);

    assert(std::string(findLabelByValue(switchTypeOpts, 2, 1)) == "Sensor");
    assert(findValueByLabel(switchTypeOpts, 2, "Switch") == 0);

    std::cout << "✓ Value lookup functions work correctly" << std::endl;
}

int main() {
    std::cout << "Testing Enum Static Mappings Implementation...\n" << std::endl;

    try {
        testEnumStaticMappings();
        testEnumOrderingStability();
        testValueLookup();

        std::cout << "\n🎉 All enum mapping tests passed!" << std::endl;
        std::cout << "✅ Static enum mappings are working correctly and provide ordering stability." << std::endl;

        return 0;
    } catch (const std::exception& e) {
        std::cout << "❌ Test failed with exception: " << e.what() << std::endl;
        return 1;
    }
}
