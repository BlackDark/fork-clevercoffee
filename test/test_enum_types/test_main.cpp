#include <iostream>
#include <string>

// Quick test to see what the enum type values should be
enum class ParamType { BOOL = 0, INT = 1, UINT8 = 2, DOUBLE = 3, FLOAT = 4, STRING = 5, ENUM = 6 };

int main() {
    std::cout << "ParamType enum values:" << std::endl;
    std::cout << "BOOL = " << static_cast<int>(ParamType::BOOL) << std::endl;
    std::cout << "INT = " << static_cast<int>(ParamType::INT) << std::endl;
    std::cout << "UINT8 = " << static_cast<int>(ParamType::UINT8) << std::endl;
    std::cout << "DOUBLE = " << static_cast<int>(ParamType::DOUBLE) << std::endl;
    std::cout << "FLOAT = " << static_cast<int>(ParamType::FLOAT) << std::endl;
    std::cout << "STRING = " << static_cast<int>(ParamType::STRING) << std::endl;
    std::cout << "ENUM = " << static_cast<int>(ParamType::ENUM) << std::endl;

    std::cout << "\nSo when your API returns type=6, it means ENUM" << std::endl;

    return 0;
}
