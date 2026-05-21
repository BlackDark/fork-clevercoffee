import { useEffect, useRef } from "react";
import { useLocation } from "react-router-dom";
import { toast } from "sonner";
import { useCleverCoffee } from "@/context/useCleverCoffee";

/**
 * Global machine alerts as bottom-right toasts (does not cover the nav bar).
 * Skipped on the home page — HomeStandbyAlert and HomeMaintenanceCard cover that there.
 */
export function MachineStatusToasts() {
  const location = useLocation();
  const onHome = location.pathname === "/";
  const { wakeFromStandby, machineStatus: status } = useCleverCoffee();
  const backflushDismissed = useRef(false);
  const standbyDismissed = useRef(false);
  const backflushToastActive = useRef(false);
  const standbyToastActive = useRef(false);

  useEffect(() => {
    if (onHome) {
      backflushToastActive.current = false;
      standbyToastActive.current = false;
      toast.dismiss("backflush-reminder");
      toast.dismiss("machine-standby");
      return;
    }

    if (!status) {
      backflushToastActive.current = false;
      toast.dismiss("backflush-reminder");
      return;
    }

    if (!status.backflushReminderDue) {
      backflushDismissed.current = false;
      backflushToastActive.current = false;
      toast.dismiss("backflush-reminder");
      return;
    }

    if (backflushDismissed.current || backflushToastActive.current) {
      return;
    }

    const shots = status.shotsSinceBackflush ?? 0;
    const threshold = status.backflushReminderThreshold ?? 50;

    backflushToastActive.current = true;
    toast.warning("Backflush recommended", {
      id: "backflush-reminder",
      description: `${shots}/${threshold} shots since last backflush. Run a detergent backflush when convenient.`,
      duration: Number.POSITIVE_INFINITY,
      closeButton: true,
      onDismiss: () => {
        backflushDismissed.current = true;
        backflushToastActive.current = false;
      },
    });
  }, [
    onHome,
    status?.backflushReminderDue,
    status?.shotsSinceBackflush,
    status?.backflushReminderThreshold,
    status,
  ]);

  useEffect(() => {
    if (onHome) {
      return;
    }

    if (!status) {
      standbyToastActive.current = false;
      toast.dismiss("machine-standby");
      return;
    }

    if (!status.isStandby) {
      standbyDismissed.current = false;
      standbyToastActive.current = false;
      toast.dismiss("machine-standby");
      return;
    }

    if (standbyDismissed.current || standbyToastActive.current) {
      return;
    }

    standbyToastActive.current = true;
    toast.warning("Machine in standby", {
      id: "machine-standby",
      description:
        "Heater is off to save energy. Wake the machine to resume normal operation.",
      duration: Number.POSITIVE_INFINITY,
      closeButton: true,
      onDismiss: () => {
        standbyDismissed.current = true;
        standbyToastActive.current = false;
      },
      action: {
        label: "Wake",
        onClick: async () => {
          const ok = await wakeFromStandby();
          if (ok) {
            standbyDismissed.current = false;
            standbyToastActive.current = false;
            toast.dismiss("machine-standby");
            toast.success("Machine waking up");
          } else {
            toast.error("Could not wake machine");
          }
        },
      },
    });
  }, [onHome, status?.isStandby, status, wakeFromStandby]);

  return null;
}
