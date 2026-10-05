/* U8g2 host shim — enough of U8g2 to render a 128x64 frame on the host.
 *
 * The R2-10 parity oracle links THIS (plus the real U8g2 sources from
 * `.pio/libdeps/esp32_usb/U8g2/src/clib`, byte-identical to upstream 2.36.18)
 * and renders the same screen to a PPM, which `tests/parity.rs` then diffs
 * against the Rust renderer. Same precedent as
 * `crates/cc-domain/tools/pid_oracle/`: the real library, a shim for the
 * Arduino platform, and a checked-in expected artefact.
 *
 * The Arduino `Print` base class (U8G2 inherits it) is the only thing that
 * does not exist off-device, so it is reimplemented here -- byte for byte
 * against the Arduino-ESP32 `Print` contract that `U8g2lib.h:326-375`
 * already assumes, and against `WString.h` for the `String` methods the OTA
 * screen uses.
 *
 * NOT part of the firmware. This file is never compiled into the device.
 */

#include "U8g2Shim.h"

#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* --------------------------------------------------------------- Arduino String */

String::String(const char* s) {
    if (s) {
        buf = (char*)malloc(strlen(s) + 1);
        strcpy(buf, s);
    } else {
        buf = (char*)malloc(1);
        buf[0] = '\0';
    }
}

String::String(const String& other) {
    buf = (char*)malloc(strlen(other.buf) + 1);
    strcpy(buf, other.buf);
}

String::String(char c) {
    buf = (char*)malloc(2);
    buf[0] = c;
    buf[1] = '\0';
}

String::~String() {
    free(buf);
}

String& String::operator=(const String& rhs) {
    if (this != &rhs) {
        char* n = (char*)malloc(strlen(rhs.buf) + 1);
        strcpy(n, rhs.buf);
        free(buf);
        buf = n;
    }
    return *this;
}

String& String::operator=(const char* rhs) {
    char*     n   = (char*)malloc(strlen(rhs) + 1);
    strcpy(n, rhs);
    free(buf);
    buf = n;
    return *this;
}

bool String::operator==(const String& rhs) const {
    return strcmp(buf, rhs.buf) == 0;
}

bool String::operator==(const char* rhs) const {
    return strcmp(buf, rhs) == 0;
}

size_t String::length() const {
    return strlen(buf);
}

bool String::isEmpty() const {
    return buf[0] == '\0';
}

void String::remove(size_t index) {
    memmove(buf + index, buf + index + 1, strlen(buf + index + 1));
}

void String::remove(unsigned int index) {
    remove((size_t)index);
}

bool String::endsWith(const String& suffix) const {
    const size_t n = suffix.length();
    if (n > length()) {
        return false;
    }
    return strcmp(buf + length() - n, suffix.buf) == 0;
}

String String::substring(unsigned int from) const {
    return String(buf + from);
}

String String::substring(unsigned int from, unsigned int to) const {
    const size_t n = to - from;
    char*        b = (char*)malloc(n + 1);
    memcpy(b, buf + from, n);
    b[n] = '\0';
    String out(b);
    free(b);
    return out;
}

/* --------------------------------------------------------------------- Print */

size_t Print::print(const char* s) {
    return write((const uint8_t*)s, strlen(s));
}

size_t Print::print(const String& s) {
    return print(s.c_str());
}

size_t Print::print(const __FlashStringHelper*) {
    return 0;
}

size_t Print::print(char c) {
    return write((uint8_t)c);
}

size_t Print::print(unsigned char n, int base) {
    return printNumber((unsigned long)n, base);
}

size_t Print::print(int n, int base) {
    return printNumber((long)n, base);
}

size_t Print::print(unsigned int n, int base) {
    return printNumber((unsigned long)n, base);
}

size_t Print::print(long n, int base) {
    return printNumber(n, base);
}

size_t Print::print(unsigned long n, int base) {
    return printNumber(n, base);
}

size_t Print::print(double n, int digits) {
    /* Exactly Arduino's Print::print(double, int): build the string with
     * snprintf("%.Nf") and write it. This matters for parity -- the C++
     * templates' number formatting goes through here, and Rust must match
     * `%.1f` rounding, which the parity test checks. */
    char  buf[64];
    int   n = snprintf(buf, sizeof(buf), "%.*f", digits, n);
    return write((const uint8_t*)buf, (size_t)n);
}

size_t Print::println(const char* s) {
    return print(s) + println();
}

size_t Print::println(void) {
    return print("\n");
}

size_t Print::printNumber(unsigned long n, uint8_t base) {
    char buf[8 * sizeof(long) + 1];
    char* p   = buf + sizeof(buf) - 1;
    *p        = '\0';
    unsigned long v = n;
    if (base < 2) {
        base = 10;
    }
    if (v == 0) {
        *--p = '0';
    } else {
        while (v) {
            const unsigned long m = v / base;
            *--p                 = (char)("0123456789abcdef"[v - m * base]);
            v                    = m;
        }
    }
    return print(p);
}

size_t Print::printNumber(long n, uint8_t base) {
    if (base == 10 && n < 0) {
        const size_t t = print('-');
        return printNumber((unsigned long)-n, base) + t;
    }
    return printNumber((unsigned long)n, base);
}
