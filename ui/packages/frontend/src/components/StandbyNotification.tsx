import { useState, useEffect } from "react";
import { AlertTriangle, Power, Loader2, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useCleverCoffee } from "@/context/useCleverCoffee";
import { toast } from "sonner";

export function StandbyNotification() {
  const { wakeFromStandby } = useCleverCoffee();
  const [isWaking, setIsWaking] = useState(false);
  const [isVisible, setIsVisible] = useState(false);
  const [lastStandbyState, setLastStandbyState] = useState<boolean | null>(null);

  useEffect(() => {
    let mounted = true;

    const checkStandby = async () => {
      try {
        const response = await fetch("/api/status", {
          signal: AbortSignal.timeout(5000),
        });
        if (!mounted) return;

        if (response.ok) {
          const data = await response.json();
          const isStandby = data.isStandby === true;

          if (isStandby !== lastStandbyState) {
            setLastStandbyState(isStandby);
            if (isStandby) {
              setIsVisible(true);
            }
          }
        }
      } catch {
        // Network errors are expected when machine is offline
      }
    };

    checkStandby();
    const interval = setInterval(checkStandby, 10000);

    return () => {
      mounted = false;
      clearInterval(interval);
    };
  }, [lastStandbyState]);

  const handleWake = async () => {
    setIsWaking(true);
    try {
      const success = await wakeFromStandby();
      if (success) {
        toast.success("Machine waking up", {
          description: "Standby timer reset",
        });
        setIsVisible(false);
        setLastStandbyState(false);
      } else {
        toast.error("Failed to wake machine", {
          description: "Could not connect to the machine",
        });
      }
    } catch {
      toast.error("Failed to wake machine", {
        description: "Connection error",
      });
    } finally {
      setIsWaking(false);
    }
  };

  const handleDismiss = () => {
    setIsVisible(false);
  };

  if (!isVisible) return null;

  return (
    <div className="fixed top-0 left-0 right-0 z-[100] animate-in slide-in-from-top duration-300">
      <div className="bg-amber-500 text-amber-950 shadow-lg border-b border-amber-600">
        <div className="container mx-auto px-4 py-3">
          <div className="flex items-center justify-between gap-4">
            <div className="flex items-center gap-3 flex-1 min-w-0">
              <AlertTriangle className="h-5 w-5 shrink-0" />
              <div className="min-w-0">
                <p className="font-semibold text-sm">
                  Machine is in Standby Mode
                </p>
                <p className="text-xs text-amber-800 truncate">
                  The machine has entered standby to save energy. Make any
                  change or click wake to resume operation.
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
                onClick={handleWake}
                disabled={isWaking}
              >
                {isWaking ? (
                  <Loader2 className="h-4 w-4 mr-1 animate-spin" />
                ) : (
                  <Power className="h-4 w-4 mr-1" />
                )}
                Wake Up
              </Button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}