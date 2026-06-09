import { createContext, useContext } from "react";
import type { CleverCoffeeContextValue } from "@/context/CleverCoffeeContext";

export const CleverCoffeeContext = createContext<
  CleverCoffeeContextValue | undefined
>(undefined);

export function useCleverCoffee() {
  const ctx = useContext(CleverCoffeeContext);
  if (!ctx)
    throw new Error(
      "useCleverCoffee must be used within a CleverCoffeeProvider",
    );
  return ctx;
}
