/**
 * @file DisplayLayoutUtils.h
 * @brief U8G2 layout helpers — fixed-width numeric fields and centered clusters
 */

#pragma once

#include "clevercoffee/defaults.h"

#include <U8g2lib.h>

namespace CleverCoffee::Display::Layout {

inline void drawStrRightInBox(U8G2* d, const int boxX, const int boxW, const int y, const char* text) {
    d->drawStr(boxX + boxW - d->getStrWidth(text), y, text);
}

inline void drawStrCenteredInBox(U8G2* d, const int boxX, const int boxW, const int y, const char* text) {
    const int x = boxX + (boxW - d->getStrWidth(text)) / 2;
    d->drawStr(x, y, text);
}

inline void drawStrCenteredOnScreen(U8G2* d, const int y, const char* text) {
    const int w = d->getStrWidth(text);
    d->drawStr((DISPLAY_WIDTH - w) / 2, y, text);
}

struct BarLabelCluster {
    int barX;
    int barY;
    int labelX;
    int labelY;
};

/** Horizontally center bar+label; vertically center both within rowTopY..rowTopY+rowH-1. */
inline BarLabelCluster layoutBarLabelCluster(U8G2*       d,
                                             const int   barW,
                                             const int   barH,
                                             const int   rowTopY,
                                             const int   rowH,
                                             const int   labelFontH,
                                             const int   gap,
                                             const char* maxLabelProbe) {
    const int labelW   = d->getStrWidth(maxLabelProbe);
    const int clusterW = barW + gap + labelW;
    const int clusterX = (DISPLAY_WIDTH - clusterW) / 2;

    BarLabelCluster layout{};
    layout.barX   = clusterX;
    layout.barY   = rowTopY + (rowH - barH) / 2;
    layout.labelX = clusterX + barW + gap;
    layout.labelY = rowTopY + (rowH - labelFontH) / 2;
    return layout;
}

} // namespace CleverCoffee::Display::Layout
