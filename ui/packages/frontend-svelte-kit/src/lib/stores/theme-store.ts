import { writable, derived } from 'svelte/store';
import { browser } from '$app/environment';

type Theme = 'dark' | 'light' | 'system';

// Initialize theme from localStorage or default to system
function initializeTheme(): Theme {
	if (browser) {
		const savedTheme = localStorage.getItem('theme') as Theme;
		return savedTheme || 'system';
	}
	return 'system';
}

// Create the theme store
export const theme = writable<Theme>(initializeTheme());

// Save theme changes to localStorage
theme.subscribe((value) => {
	if (browser) {
		localStorage.setItem('theme', value);

		// Update the document class for immediate theme change
		updateThemeClass(value);
	}
});

// Helper to update the document class based on theme
function updateThemeClass(value: Theme) {
	if (!browser) return;

	const isDark =
		value === 'dark' ||
		(value === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);

	document.documentElement.classList.toggle('dark', isDark);
}

// Derived store to determine if dark mode is active
export const isDarkMode = derived(theme, ($theme) => {
	if (!browser) return false;

	return (
		$theme === 'dark' ||
		($theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
	);
});

// Function to set the theme
export function setTheme(value: Theme) {
	theme.set(value);
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

// Initialize theme class on load
if (browser) {
	updateThemeClass(initializeTheme());

	// Listen for system theme changes
	window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
		const currentTheme = localStorage.getItem('theme') as Theme;
		if (currentTheme === 'system') {
			updateThemeClass('system');
		}
	});
}
