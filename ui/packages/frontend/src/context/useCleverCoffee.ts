import type { CleverCoffeeContextValue } from "@/context/CleverCoffeeContext";
import { createContext, useContext } from "react";

export const CleverCoffeeContext = createContext<
  CleverCoffeeContextValue | undefined
>(undefined);

export function useCleverCoffee() {
  const ctx = useContext(CleverCoffeeContext);
  if (!ctx)
    throw new Error(
      "useCleverCoffee must be used within a CleverCoffeeProvider"
    );
  return ctx;
}
