/**
 * @file DisplayOtaScreen.h
 * @brief Fullscreen OLED screen for firmware/filesystem OTA updates
 */

#pragma once

namespace CleverCoffee {
class SystemContext;
}

namespace CleverCoffee::Display {

[[nodiscard]] bool drawOtaScreen(SystemContext& systemContext);
void               refreshOtaDisplay(SystemContext& systemContext);

} // namespace CleverCoffee::Display
