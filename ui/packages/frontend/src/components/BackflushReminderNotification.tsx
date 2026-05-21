import { useState, useEffect, useCallback } from "react";
import { AlertTriangle, Droplets, X } from "lucide-react";
import { Button } from "@/components/ui/button";

interface MachineStatus {
  backflushReminderDue?: boolean;
  shotsSinceBackflush?: number;
  backflushReminderThreshold?: number;
}

export function BackflushReminderNotification() {
  const [isVisible, setIsVisible] = useState(false);
  const [status, setStatus] = useState<MachineStatus | null>(null);
  const [dismissedThisSession, setDismissedThisSession] = useState(false);

  const applyStatus = useCallback(
    (data: MachineStatus) => {
      setStatus(data);
      if (data.backflushReminderDue === true && !dismissedThisSession) {
        setIsVisible(true);
      } else if (data.backflushReminderDue !== true) {
        setIsVisible(false);
      }
    },
    [dismissedThisSession]
  );

  useEffect(() => {
    let mounted = true;

    const checkStatus = async () => {
      try {
        const response = await fetch("/api/status", {
          signal: AbortSignal.timeout(5000),
        });
        if (!mounted || !response.ok) return;

        const data = (await response.json()) as MachineStatus;
        applyStatus(data);
      } catch {
        // Network errors are expected when machine is offline
      }
    };

    checkStatus();
    const interval = setInterval(checkStatus, 10000);

    return () => {
      mounted = false;
      clearInterval(interval);
    };
  }, [applyStatus]);

  const handleDismiss = () => {
    setDismissedThisSession(true);
    setIsVisible(false);
  };

  if (!isVisible || !status?.backflushReminderDue) {
    return null;
  }

  const shots = status.shotsSinceBackflush ?? 0;
  const threshold = status.backflushReminderThreshold ?? 50;

  return (
    <div className="fixed top-16 left-0 right-0 z-50 pointer-events-none">
      <div className="pointer-events-auto bg-amber-100 border-b border-amber-300 text-amber-950 shadow-md">
        <div className="container mx-auto px-4 py-3">
          <div className="flex items-center justify-between gap-4">
            <div className="flex items-center gap-3 flex-1 min-w-0">
              <AlertTriangle className="h-5 w-5 shrink-0" />
              <div className="min-w-0">
                <p className="font-semibold text-sm">Backflush recommended</p>
                <p className="text-xs text-amber-800 truncate">
                  {shots}/{threshold} shots since last backflush. Run a detergent
                  backflush cycle when convenient.
                </p>
              </div>
            </div>
            <div className="flex items-center gap-2 shrink-0">
              <Button
                size="sm"
                variant="outline"
                className="bg-amber-100 border-amber-300 text-amber-900 hover:bg-amber-200"
                onClick={handleDismiss}
              >
                <X className="h-4 w-4 mr-1" />
                Dismiss
              </Button>
              <Button
                size="sm"
                className="bg-amber-900 text-amber-50 hover:bg-amber-800"
                asChild
              >
                <a href="/">
                  <Droplets className="h-4 w-4 mr-1" />
                  Home
                </a>
              </Button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
