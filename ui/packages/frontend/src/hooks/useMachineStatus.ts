import { useCleverCoffee } from "@/context/useCleverCoffee";

/** Shared machine status from CleverCoffeeProvider (single poller). */
export function useMachineStatus() {
  const { machineStatus, machineStatusLoading, refetchMachineStatus } =
    useCleverCoffee();

  return {
    status: machineStatus,
    loading: machineStatusLoading,
    refetch: refetchMachineStatus,
  };
}
