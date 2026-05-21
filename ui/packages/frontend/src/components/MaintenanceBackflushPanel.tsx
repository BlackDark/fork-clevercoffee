import { useCallback, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Loader2, RotateCcw } from "lucide-react";
import { toast } from "sonner";

interface MaintenanceStatus {
  shotsSinceBackflush?: number;
  backflushReminderThreshold?: number;
  backflushReminderDue?: boolean;
}

export function MaintenanceBackflushPanel() {
  const [status, setStatus] = useState<MaintenanceStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [resetting, setResetting] = useState(false);

  const fetchStatus = useCallback(async () => {
    try {
      const response = await fetch("/api/status", {
        signal: AbortSignal.timeout(5000),
      });
      if (response.ok) {
        setStatus((await response.json()) as MaintenanceStatus);
      }
    } catch {
      // ignore offline
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchStatus();
    const interval = setInterval(fetchStatus, 10000);
    return () => clearInterval(interval);
  }, [fetchStatus]);

  const handleReset = async () => {
    setResetting(true);
    try {
      const response = await fetch("/api/maintenance/reset-backflush-counter", {
        method: "POST",
      });
      if (!response.ok) {
        throw new Error("Reset failed");
      }
      const data = (await response.json()) as MaintenanceStatus;
      setStatus((prev) => ({ ...prev, ...data }));
      toast.success("Backflush counter reset");
    } catch {
      toast.error("Failed to reset backflush counter");
    } finally {
      setResetting(false);
    }
  };

  const shots = status?.shotsSinceBackflush ?? 0;
  const threshold = status?.backflushReminderThreshold ?? 50;

  return (
    <div className="mb-6 rounded-lg border bg-card p-4">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h3 className="font-semibold">Shots since last backflush</h3>
          <p className="text-sm text-muted-foreground">
            {loading ? (
              "Loading..."
            ) : (
              <>
                <span className="text-foreground font-medium">
                  {shots} / {threshold}
                </span>
                {status?.backflushReminderDue ? " — reminder active" : ""}
              </>
            )}
          </p>
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={handleReset}
          disabled={resetting || loading}
        >
          {resetting ? (
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          ) : (
            <RotateCcw className="mr-2 h-4 w-4" />
          )}
          Reset counter
        </Button>
      </div>
    </div>
  );
}
