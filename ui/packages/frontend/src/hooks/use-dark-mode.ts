// Custom useTheme hook for Vite/shadcn
import type { ThemeProviderState } from "@/types/theme";
import { createContext, useContext, useEffect, useState } from "react";

const initialState: ThemeProviderState = {
  theme: "system",
  setTheme: () => null,
};

export const ThemeProviderContext =
  createContext<ThemeProviderState>(initialState);

export const useTheme = () => {
  const context = useContext(ThemeProviderContext);

  if (context === undefined)
    throw new Error("useTheme must be used within a ThemeProvider");

  return context;
};

// This hook is for theme/dark mode only and is independent of clever coffee state.
export function useDarkMode() {
  const { theme } = useTheme();
  const [isDark, setIsDark] = useState(false);

  useEffect(() => {
    // Check if the resolved theme is dark, default to false if undefined
    const isCurrentlyDark = theme === "dark";
    setIsDark(isCurrentlyDark);
  }, [theme]);

  return isDark;
}

export function getChartTheme(isDark: boolean) {
  return {
    // Text colors for axes, labels, and legend
    textColor: isDark ? "#e5e7eb" : "#374151", // gray-200 in dark, gray-700 in light
    axisColor: isDark ? "#6b7280" : "#9ca3af", // gray-500 in dark, gray-400 in light
    gridColor: isDark ? "#374151" : "#e5e7eb", // gray-700 in dark, gray-200 in light
    backgroundColor: isDark ? "#111827" : "#ffffff", // gray-900 in dark, white in light

    // Chart series colors optimized for both themes
    series: {
      temperature: {
        current: isDark ? "#06b6d4" : "#0891b2", // cyan-500 in dark, cyan-600 in light
        target: isDark ? "#a855f7" : "#9333ea", // purple-500 in dark, purple-600 in light
      },
      heater: {
        power: isDark ? "#f59e0b" : "#d97706", // amber-500 in dark, amber-600 in light
      },
    },
  };
}
