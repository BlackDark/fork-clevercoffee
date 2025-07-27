#include "src/Config.h"
#include <iostream>

int main() {
    std::cout << "ParamType values:" << std::endl;
    std::cout << "INT: " << (int)ParamType::INT << std::endl;
    std::cout << "UINT8: " << (int)ParamType::UINT8 << std::endl;
    std::cout << "DOUBLE: " << (int)ParamType::DOUBLE << std::endl;
    std::cout << "FLOAT: " << (int)ParamType::FLOAT << std::endl;
    std::cout << "STRING: " << (int)ParamType::STRING << std::endl;
    std::cout << "ENUM: " << (int)ParamType::ENUM << std::endl;
    std::cout << "BOOL: " << (int)ParamType::BOOL << std::endl;
    return 0;
}
