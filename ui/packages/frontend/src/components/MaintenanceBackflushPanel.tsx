import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Loader2, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import { apiFetch } from "@/lib/api-config";
import { API_ROUTES } from "@/lib/routes";
import { useMachineStatus } from "@/hooks/useMachineStatus";

export function MaintenanceBackflushPanel() {
  const { status, loading, refetch } = useMachineStatus();
  const [resetting, setResetting] = useState(false);

  const handleReset = async () => {
    setResetting(true);
    try {
      const response = await apiFetch(API_ROUTES.MAINTENANCE_RESET_BACKFLUSH, {
        method: "POST",
      });
      if (!response.ok) {
        throw new Error("Reset failed");
      }
      await refetch();
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
