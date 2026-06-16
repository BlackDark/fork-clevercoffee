#include "clevercoffee/ConfigJson.h"

#include <cstring>

namespace {

constexpr size_t kMaxPathSegment = 64;

bool copyPathSegment(const char* dotPath, char* segment, size_t segmentSize, const char** rest) noexcept {
    if (dotPath == nullptr || segment == nullptr || segmentSize == 0) {
        return false;
    }

    const char* dot = std::strchr(dotPath, '.');
    const size_t  len = dot ? static_cast<size_t>(dot - dotPath) : std::strlen(dotPath);
    if (len == 0 || len >= segmentSize) {
        return false;
    }

    std::memcpy(segment, dotPath, len);
    segment[len] = '\0';
    if (rest != nullptr) {
        *rest = dot ? dot + 1 : dotPath + len;
    }

    return true;
}

JsonObject ensureObjectChild(JsonObject parent, const char* key) noexcept {
    if (!parent[key].is<JsonObject>()) {
        parent[key].to<JsonObject>();
    }
    return parent[key].as<JsonObject>();
}

} // namespace

namespace CleverCoffee::ConfigJson {

bool usesFlatDotKeys(const JsonObjectConst root) noexcept {
    if (root.isNull()) {
        return false;
    }

    for (JsonPairConst kv : root) {
        if (std::strchr(kv.key().c_str(), '.') != nullptr) {
            return true;
        }
    }

    return false;
}

JsonVariantConst getNested(const JsonObjectConst root, const char* dotPath) noexcept {
    if (root.isNull() || dotPath == nullptr || dotPath[0] == '\0') {
        return JsonVariantConst{};
    }

    JsonObjectConst current = root;
    const char*     cursor  = dotPath;

    while (true) {
        char        segment[kMaxPathSegment];
        const char* rest = nullptr;
        if (!copyPathSegment(cursor, segment, sizeof(segment), &rest)) {
            return JsonVariantConst{};
        }

        if (rest[0] == '\0') {
            return current[segment];
        }

        JsonVariantConst child = current[segment];
        if (!child.is<JsonObjectConst>()) {
            return JsonVariantConst{};
        }

        current = child.as<JsonObjectConst>();
        cursor  = rest;
    }
}

bool setNested(const JsonObject root, const char* dotPath, const JsonVariant value) noexcept {
    if (root.isNull() || dotPath == nullptr || dotPath[0] == '\0') {
        return false;
    }

    JsonObject  current = root;
    const char* cursor  = dotPath;

    while (true) {
        char        segment[kMaxPathSegment];
        const char* rest = nullptr;
        if (!copyPathSegment(cursor, segment, sizeof(segment), &rest)) {
            return false;
        }

        if (rest[0] == '\0') {
            current[segment] = value;
            return true;
        }

        current = ensureObjectChild(current, segment);
        cursor  = rest;
    }
}

} // namespace CleverCoffee::ConfigJson
