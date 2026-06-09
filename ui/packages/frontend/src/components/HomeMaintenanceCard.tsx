import { AlertTriangle, Droplets, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Skeleton } from "@/components/ui/skeleton";
import { useCleverCoffee } from "@/context/useCleverCoffee";
import { useMachineStatus } from "@/hooks/useMachineStatus";
import { apiFetch } from "@/lib/api-config";
import { API_ROUTES } from "@/lib/routes";

export function HomeMaintenanceCard() {
  const { status, loading, refetch } = useMachineStatus();
  const { toggleBackflush } = useCleverCoffee();

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <Skeleton className="h-6 w-40" />
        </CardHeader>
        <CardContent>
          <Skeleton className="h-10 w-32" />
        </CardContent>
      </Card>
    );
  }

  if (!status) {
    return null;
  }

  const shots = status.shotsSinceBackflush ?? 0;
  const threshold = status.backflushReminderThreshold ?? 50;
  const progress = threshold > 0 ? Math.min(100, (shots / threshold) * 100) : 0;
  const due = status.backflushReminderDue === true;

  const handleReset = async () => {
    try {
      const response = await apiFetch(API_ROUTES.MAINTENANCE_RESET_BACKFLUSH, {
        method: "POST",
      });
      if (!response.ok) throw new Error("reset failed");
      await refetch();
      toast.success("Counter reset");
    } catch {
      toast.error("Failed to reset counter");
    }
  };

  const handleBackflush = async () => {
    const result = await toggleBackflush();
    if (result.success) toast.success("Backflush mode toggled");
    else
      toast.error("Failed to toggle backflush", {
        description:
          result.error ?? "Please check your connection and try again.",
      });
  };

  return (
    <Card className={due ? "border-amber-400/60" : undefined}>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <div
            className={`flex h-10 w-10 items-center justify-center rounded-lg ${
              due ? "bg-amber-500/15" : "bg-emerald-500/10"
            }`}
          >
            <Droplets
              className={`h-5 w-5 ${due ? "text-amber-600" : "text-emerald-600"}`}
            />
          </div>
          Maintenance
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="space-y-2">
          <div className="flex items-end justify-between gap-2">
            <div>
              <p className="text-sm text-muted-foreground">
                Shots since backflush
              </p>
              <p className="text-3xl font-bold tracking-tight">
                {shots}
                <span className="text-lg font-normal text-muted-foreground">
                  {" "}
                  / {threshold}
                </span>
              </p>
            </div>
            <span
              className={`text-xs font-medium px-2 py-1 rounded-full ${
                due
                  ? "bg-amber-100 text-amber-900 dark:bg-amber-950 dark:text-amber-100"
                  : "bg-muted text-muted-foreground"
              }`}
            >
              {due ? "Clean due" : `${Math.max(0, threshold - shots)} left`}
            </span>
          </div>
          <Progress
            value={progress}
            className={due ? "[&>div]:bg-amber-500" : undefined}
          />
        </div>

        {due && (
          <Alert className="border-amber-300 bg-amber-50 text-amber-950 dark:border-amber-700 dark:bg-amber-950/40 dark:text-amber-50">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>Backflush recommended</AlertTitle>
            <AlertDescription>
              Run a detergent backflush cycle, then turn backflush mode off or
              reset the counter when finished.
            </AlertDescription>
          </Alert>
        )}

        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant={due ? "default" : "outline"}
            onClick={handleBackflush}
          >
            <Droplets className="h-4 w-4 mr-1" />
            Backflush
          </Button>
          <Button size="sm" variant="outline" onClick={handleReset}>
            <RotateCcw className="h-4 w-4 mr-1" />
            Reset counter
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
