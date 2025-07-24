import { browser } from '$app/environment';

export type Theme = 'dark' | 'light' | 'system';

// Initialize theme from localStorage or default to system
function initializeTheme(): Theme {
	if (browser) {
		const savedTheme = localStorage.getItem('theme') as Theme;
		return savedTheme || 'system';
	}
	return 'system';
}

// Helper to update the document class based on theme
export function updateThemeClass(value: Theme) {
	if (!browser) return;

	const isDark =
		value === 'dark' ||
		(value === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);

	document.documentElement.classList.toggle('dark', isDark);
}

// Create reactive theme state
let theme = $state<Theme>(initializeTheme());

// Reactive derived value for dark mode
const isDarkMode = $derived.by(() => {
	if (!browser) return false;

	return (
		theme === 'dark' ||
		(theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
	);
});

// Function to set the theme
export function setTheme(value: Theme) {
	theme = value;
}

// Getter function to access current theme
export function getTheme(): Theme {
	return theme;
}

// Getter function to access dark mode state
export function getIsDarkMode(): boolean {
	return isDarkMode;
}

// Chart theme helper
export function getChartTheme(isDark: boolean) {
	return {
		// Text colors for axes, labels, and legend
		textColor: isDark ? '#e5e7eb' : '#374151', // gray-200 in dark, gray-700 in light
		axisColor: isDark ? '#6b7280' : '#9ca3af', // gray-500 in dark, gray-400 in light
		gridColor: isDark ? '#374151' : '#e5e7eb', // gray-700 in dark, gray-200 in light
		backgroundColor: isDark ? '#111827' : '#ffffff', // gray-900 in dark, white in light

		// Chart series colors optimized for both themes
		series: {
			temperature: {
				current: isDark ? '#06b6d4' : '#0891b2', // cyan-500 in dark, cyan-600 in light
				target: isDark ? '#a855f7' : '#9333ea' // purple-500 in dark, purple-600 in light
			},
			heater: {
				power: isDark ? '#f59e0b' : '#d97706' // amber-500 in dark, amber-600 in light
			}
		}
	};
}
