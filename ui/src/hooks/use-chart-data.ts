import { useState, useCallback } from "react";

export interface ChartDataPoint {
  timestamp: number;
  value: number;
}

export interface TemperatureData {
  currentTemp: number;
  targetTemp: number;
  heaterPower?: number;
}

export interface HistoryData {
  currentTemps: number[];
  targetTemps: number[];
  heaterPowers: number[];
}

const MAX_VALUES = 600;

export function useChartData() {
  const [curTempVals, setCurTempVals] = useState<number[]>([]);
  const [targetTempVals, setTargetTempVals] = useState<number[]>([]);
  const [heaterPowerVals, setHeaterPowerVals] = useState<number[]>([]);
  const [tempDates, setTempDates] = useState<Date[]>([]);
  const [heaterDates, setHeaterDates] = useState<Date[]>([]);

  const addTempData = useCallback(
    (jsonValue: TemperatureData | HistoryData, isSingleValue = false) => {
      if (isSingleValue) {
        const data = jsonValue as TemperatureData;

        setCurTempVals((prev) => {
          const newVals = [...prev, data.currentTemp];
          return newVals.length > MAX_VALUES
            ? newVals.slice(-MAX_VALUES)
            : newVals;
        });

        setTargetTempVals((prev) => {
          const newVals = [...prev, data.targetTemp];
          return newVals.length > MAX_VALUES
            ? newVals.slice(-MAX_VALUES)
            : newVals;
        });

        setTempDates((prev) => {
          const newDates = [...prev, new Date()];
          return newDates.length > MAX_VALUES
            ? newDates.slice(-MAX_VALUES)
            : newDates;
        });
      } else {
        const data = jsonValue as HistoryData;

        // Set data lists to values from history json
        setCurTempVals([...data.currentTemps]);
        setTargetTempVals([...data.targetTemps]);

        // Create dates for all history values (3 seconds between each value)
        const newTempDates: Date[] = [];
        for (let i = data.currentTemps.length; i > 0; i--) {
          const date = new Date();
          date.setSeconds(date.getSeconds() - 3 * i);
          newTempDates.push(date);
        }
        setTempDates(newTempDates);
      }
    },
    [] // No dependencies needed as we use functional state updates
  );

  const addHeaterData = useCallback(
    (jsonValue: TemperatureData | HistoryData, isSingleValue = false) => {
      if (isSingleValue) {
        const data = jsonValue as TemperatureData;

        if (data.heaterPower !== undefined) {
          setHeaterPowerVals((prev) => {
            const newVals = [...prev, data.heaterPower!];
            return newVals.length > MAX_VALUES
              ? newVals.slice(-MAX_VALUES)
              : newVals;
          });

          setHeaterDates((prev) => {
            const newDates = [...prev, new Date()];
            return newDates.length > MAX_VALUES
              ? newDates.slice(-MAX_VALUES)
              : newDates;
          });
        }
      } else {
        const data = jsonValue as HistoryData;

        setHeaterPowerVals([...data.heaterPowers]);

        // Create dates for all history values (3 seconds between each value)
        const newHeaterDates: Date[] = [];
        for (let i = data.heaterPowers.length; i > 0; i--) {
          const date = new Date();
          date.setSeconds(date.getSeconds() - 3 * i);
          newHeaterDates.push(date);
        }
        setHeaterDates(newHeaterDates);
      }
    },
    [] // No dependencies needed as we use functional state updates
  );

  const clearData = useCallback(() => {
    setCurTempVals([]);
    setTargetTempVals([]);
    setHeaterPowerVals([]);
    setTempDates([]);
    setHeaterDates([]);
  }, []);

  return {
    addTempData,
    addHeaterData,
    clearData,
    tempData: { curTempVals, targetTempVals, tempDates },
    heaterData: { heaterPowerVals, heaterDates },
  };
}
