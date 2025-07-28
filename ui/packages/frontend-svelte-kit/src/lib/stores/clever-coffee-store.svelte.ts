import { apiFetch, SERVER_BASE_URL } from '../api-config';
import type { Parameter, UpdateParameter } from '../types/parameters';
import { groupParametersBySection } from '../parameter-utils';
import { ensureCompleteParameters } from '../parameter-metadata';
import { SvelteDate, SvelteURLSearchParams } from 'svelte/reactivity';

// Define types for API responses
interface HistoryData {
	currentTemps: number[];
	targetTemps: number[];
	heaterPowers: number[];
}

interface TemperatureData {
	currentTemp: number;
	targetTemp: number;
	heaterPower: number;
}

// Main state type
interface CleverCoffeeState {
	// Parameters
	parameters: Parameter[];
	loadingParams: boolean;
	errorParams: string | null;

	// Chart data
	tempData: {
		tempDates: Date[];
		curTempVals: number[];
		targetTempVals: number[];
	};
	heaterData: {
		heaterDates: Date[];
		heaterPowerVals: number[];
	};
	chartError: string | null;
	isHistoryLoaded: boolean;

	// Health
	isOnline: boolean;
	lastHealthCheck: Date | null;
	connectionError: string | null;

	// Temperature
	currentTempData: TemperatureData | null;
	isLoadingTemp: boolean;
	temperatureError: string | null;
}

// Main state
export const cleverCoffeeState = $state<CleverCoffeeState>({
	// Parameters
	parameters: [],
	loadingParams: true,
	errorParams: null,

	// Chart data
	tempData: { tempDates: [], curTempVals: [], targetTempVals: [] },
	heaterData: { heaterDates: [], heaterPowerVals: [] },
	chartError: null,
	isHistoryLoaded: false,

	// Health
	isOnline: true,
	lastHealthCheck: null,
	connectionError: null,

	// Temperature
	currentTempData: null,
	isLoadingTemp: true,
	temperatureError: null
});

// Derived state
const derivedParametersBySection = $derived(groupParametersBySection(cleverCoffeeState.parameters));
export const parametersBySection = () => derivedParametersBySection;

// Helper functions
export function addTempData(data: { currentTemp?: number; targetTemp?: number }) {
	if (data.currentTemp != null && data.targetTemp != null) {
		const now = new SvelteDate();
		cleverCoffeeState.tempData.tempDates.push(now);
		cleverCoffeeState.tempData.curTempVals.push(data.currentTemp);
		cleverCoffeeState.tempData.targetTempVals.push(data.targetTemp);
	}
}

export function addHeaterData(data: { heaterPower?: number }) {
	if (data.heaterPower !== undefined) {
		const now = new SvelteDate();
		cleverCoffeeState.heaterData.heaterDates.push(now);
		cleverCoffeeState.heaterData.heaterPowerVals.push(data.heaterPower);
	}
}

// API functions
export async function fetchParameters(refresh = true) {
	try {
		if (refresh) {
			cleverCoffeeState.loadingParams = true;
			cleverCoffeeState.errorParams = null;
		}

		const response = await apiFetch('/parameters?filter=all');
		if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);

		const data = await response.json();
		// Merge with metadata/defaults
		cleverCoffeeState.parameters.splice(
			0,
			cleverCoffeeState.parameters.length,
			...ensureCompleteParameters(data).sort((a, b) => a.name.localeCompare(b.name))
		);
		cleverCoffeeState.errorParams = null;
	} catch (err) {
		cleverCoffeeState.errorParams =
			err instanceof Error ? err.message : 'Failed to fetch parameters';
	} finally {
		cleverCoffeeState.loadingParams = false;
	}
}

export async function fetchHistoryData(): Promise<boolean> {
	try {
		cleverCoffeeState.chartError = null;
		const response = await apiFetch('/history');
		if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);

		const historyData: HistoryData = await response.json();

		console.log('[fetchHistoryData] historyData:', historyData);

		const dates: Date[] = [];

		for (let i = historyData.heaterPowers.length; i > 0; i--) {
			const date = new SvelteDate();
			date.setSeconds(date.getSeconds() - 3 * i);
			dates.push(date);
		}

		cleverCoffeeState.tempData.tempDates.splice(
			0,
			cleverCoffeeState.tempData.tempDates.length,
			...dates
		);
		cleverCoffeeState.tempData.curTempVals.splice(
			0,
			cleverCoffeeState.tempData.curTempVals.length,
			...historyData.currentTemps
		);
		cleverCoffeeState.tempData.targetTempVals.splice(
			0,
			cleverCoffeeState.tempData.targetTempVals.length,
			...historyData.targetTemps
		);

		cleverCoffeeState.heaterData.heaterDates.splice(
			0,
			cleverCoffeeState.heaterData.heaterDates.length,
			...dates
		);
		cleverCoffeeState.heaterData.heaterPowerVals.splice(
			0,
			cleverCoffeeState.heaterData.heaterPowerVals.length,
			...historyData.heaterPowers
		);

		cleverCoffeeState.isHistoryLoaded = true;
		return true;
	} catch (err) {
		cleverCoffeeState.chartError = 'Failed to load chart history data';
		console.error('[fetchHistoryData] error:', err);
		cleverCoffeeState.isHistoryLoaded = false;
		return false;
	}
}

export function updateParameter(name: string, value: string | number | boolean) {
	const paramIndex = cleverCoffeeState.parameters.findIndex((param) => param.name === name);
	if (paramIndex >= 0) {
		cleverCoffeeState.parameters[paramIndex] = {
			...cleverCoffeeState.parameters[paramIndex],
			value
		};
	}
}

export async function saveParameters(changedParams?: UpdateParameter[]): Promise<boolean> {
	try {
		cleverCoffeeState.errorParams = null;
		const formData = new SvelteURLSearchParams();

		if (changedParams) {
			changedParams.forEach((param) => {
				formData.append(param.name, String(param.value));
			});
		} else {
			// Save all parameters
			cleverCoffeeState.parameters.forEach((param) => {
				formData.append(param.name, String(param.value));
			});
		}
		const response = await apiFetch('/parameters', {
			method: 'POST',
			headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
			body: formData.toString()
		});

		if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);
		await fetchParameters();
		return true;
	} catch (err) {
		cleverCoffeeState.errorParams =
			err instanceof Error ? err.message : 'Failed to save parameters';
		return false;
	}
}

export function getParameter(name: string): Parameter | undefined {
	return cleverCoffeeState.parameters.find((param) => param.name === name);
}

export async function checkHealth(): Promise<boolean> {
	try {
		const controller = new AbortController();
		const timeoutId = setTimeout(() => controller.abort(), 3000);

		const response = await apiFetch('/health', {
			method: 'GET',
			signal: controller.signal
		});

		clearTimeout(timeoutId);
		const isHealthy = response.ok;
		cleverCoffeeState.isOnline = isHealthy;
		cleverCoffeeState.lastHealthCheck = new SvelteDate();
		cleverCoffeeState.connectionError = isHealthy ? null : 'Service unavailable';
		return isHealthy;
	} catch {
		cleverCoffeeState.isOnline = false;
		cleverCoffeeState.lastHealthCheck = new SvelteDate();
		cleverCoffeeState.connectionError = 'Connection failed';
		return false;
	}
}

export async function fetchTemperatureAndChartData(showLoading = false): Promise<boolean> {
	try {
		cleverCoffeeState.temperatureError = null;
		if (showLoading) {
			cleverCoffeeState.isLoadingTemp = true;
		}

		const controller = new AbortController();
		const timeoutId = setTimeout(() => controller.abort(), 5000);

		const response = await apiFetch('/temperatures', {
			signal: controller.signal
		});

		clearTimeout(timeoutId);
		if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`);

		const tempDataResponse: TemperatureData = await response.json();

		cleverCoffeeState.currentTempData = tempDataResponse;
		cleverCoffeeState.temperatureError = null;
		cleverCoffeeState.isLoadingTemp = false;

		// Only append if history has loaded
		if (cleverCoffeeState.isHistoryLoaded) {
			addTempData({
				currentTemp: tempDataResponse.currentTemp,
				targetTemp: tempDataResponse.targetTemp
			});

			addHeaterData({ heaterPower: tempDataResponse.heaterPower });
		}
		return true;
	} catch (err) {
		cleverCoffeeState.currentTempData = null;
		cleverCoffeeState.temperatureError = 'Temperature sensor offline';
		if (showLoading) {
			cleverCoffeeState.isLoadingTemp = false;
		}
		console.error('[fetchTemperatureAndChartData] error:', err);
		return false;
	}
}

// Event source connection
let eventSource: EventSource | null = null;
let retryTimeout: number | null = null;

export function connectEventSource() {
	const startEventSource = () => {
		if (eventSource) {
			eventSource.close();
		}

		eventSource = new EventSource(`${SERVER_BASE_URL}/events`);

		eventSource.onopen = () => {
			cleverCoffeeState.isOnline = true;
			cleverCoffeeState.connectionError = null;
			cleverCoffeeState.temperatureError = null;
		};

		eventSource.addEventListener('new_temps', (e) => {
			try {
				const tempDataResponse: TemperatureData = JSON.parse(e.data);
				cleverCoffeeState.currentTempData = tempDataResponse;
				cleverCoffeeState.temperatureError = null;
				cleverCoffeeState.isLoadingTemp = false;

				if (cleverCoffeeState.isHistoryLoaded) {
					addTempData({
						currentTemp: tempDataResponse.currentTemp,
						targetTemp: tempDataResponse.targetTemp
					});

					addHeaterData({ heaterPower: tempDataResponse.heaterPower });
				}
			} catch (err: unknown) {
				console.log('[EventSource] Error parsing temperature event:', err);
				cleverCoffeeState.temperatureError = 'Failed to parse temperature event';
				cleverCoffeeState.currentTempData = null;
			}
		});

		eventSource.addEventListener('weight', (e) => {
			try {
				console.log(e.data);
			} catch (err: unknown) {
				console.log('[EventSource] Error parsing weight event:', err);
			}
		});

		eventSource.onerror = () => {
			cleverCoffeeState.connectionError = 'Lost connection to event source';
			cleverCoffeeState.isOnline = false;
			cleverCoffeeState.temperatureError = 'Lost connection';
			cleverCoffeeState.currentTempData = null;

			if (eventSource) {
				eventSource.close();
				eventSource = null;
			}

			// Retry after 3 seconds
			if (retryTimeout) {
				clearTimeout(retryTimeout);
			}
			retryTimeout = window.setTimeout(startEventSource, 3000);
		};
	};

	startEventSource();

	return {
		disconnect: () => {
			if (eventSource) {
				eventSource.close();
				eventSource = null;
			}
			if (retryTimeout) {
				clearTimeout(retryTimeout);
				retryTimeout = null;
			}
		}
	};
}

export function retryConnection() {
	cleverCoffeeState.connectionError = null;
	fetchTemperatureAndChartData(true);
	fetchParameters();
	connectEventSource();
}

// Dedicated toggle functions
export async function togglePid(): Promise<boolean> {
	try {
		const response = await apiFetch('/pid', { method: 'POST' });
		if (response.ok) {
			await fetchParameters(false);
			return true;
		}
		return false;
	} catch {
		return false;
	}
}

export async function toggleSteam(): Promise<boolean> {
	try {
		const response = await apiFetch('/steam', { method: 'POST' });
		if (response.ok) {
			await fetchParameters(false);
			return true;
		}
		return false;
	} catch {
		return false;
	}
}

export async function toggleBackflush(): Promise<boolean> {
	try {
		const response = await apiFetch('/backflush', { method: 'POST' });
		if (response.ok) {
			await fetchParameters(false);
			return true;
		}
		return false;
	} catch {
		return false;
	}
}

export async function toggleTareScale(): Promise<boolean> {
	try {
		const response = await apiFetch('/scale/tare', { method: 'POST' });
		if (response.ok) {
			await fetchParameters(false);
			return true;
		}
		return false;
	} catch {
		return false;
	}
}

export async function toggleScaleCalibration(): Promise<boolean> {
	try {
		const response = await apiFetch('/scale/calibration', { method: 'POST' });
		if (response.ok) {
			await fetchParameters(false);
			return true;
		}
		return false;
	} catch {
		return false;
	}
}

// Initialize data on import
fetchParameters();
