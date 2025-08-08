// test/test_cpp_features/test_cpp_features.cpp
#include <unity.h>

#if defined(ARDUINO)
#include <Arduino.h>
#endif

// Feature detection macros (same as before)
#ifdef __cpp_concepts
    #define HAS_CONCEPTS 1
#else
    #define HAS_CONCEPTS 0
#endif

#ifdef __cpp_lib_expected
    #define HAS_EXPECTED 1
#else
    #define HAS_EXPECTED 0
#endif

#ifdef __cpp_lib_format
    #define HAS_FORMAT 1
#else
    #define HAS_FORMAT 0
#endif

void test_concepts_support(void) {
    TEST_ASSERT_EQUAL_INT(1, HAS_CONCEPTS);
}

void test_expected_support(void) {
    TEST_ASSERT_EQUAL_INT(1, HAS_EXPECTED);
}

void test_format_support(void) {
    TEST_ASSERT_EQUAL_INT(1, HAS_FORMAT);
}

#if defined(ARDUINO)
void setup() {
    Serial.begin(115200);
    delay(2000); // Wait for serial

    UNITY_BEGIN();
    RUN_TEST(test_concepts_support);
    RUN_TEST(test_expected_support);
    RUN_TEST(test_format_support);
    UNITY_END();
}

void loop() {
    // Keep empty
}
#else
int main() {
    UNITY_BEGIN();
    RUN_TEST(test_concepts_support);
    RUN_TEST(test_expected_support);
    RUN_TEST(test_format_support);
    UNITY_END();
    return 0;
}
#endif
