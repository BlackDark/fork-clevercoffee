import { Moon, Power } from "lucide-react";
import { toast } from "sonner";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useCleverCoffee } from "@/context/useCleverCoffee";
import { useMachineStatus } from "@/hooks/useMachineStatus";

export function HomeStandbyAlert() {
  const { status, loading } = useMachineStatus();
  const { wakeFromStandby } = useCleverCoffee();

  if (loading) {
    return <Skeleton className="h-20 w-full rounded-lg" />;
  }

  if (!status?.isStandby) {
    return null;
  }

  const handleWake = async () => {
    const ok = await wakeFromStandby();
    if (ok) toast.success("Machine waking up");
    else toast.error("Could not wake machine");
  };

  return (
    <Alert className="border-indigo-300 bg-indigo-50 text-indigo-950 dark:border-indigo-700 dark:bg-indigo-950/40 dark:text-indigo-50">
      <Moon className="h-4 w-4" />
      <AlertTitle>Standby mode active</AlertTitle>
      <AlertDescription className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <span>
          The machine is in standby to save energy. Heating is reduced until you
          wake it or start using the machine again.
        </span>
        <Button
          size="sm"
          variant="outline"
          className="shrink-0"
          onClick={handleWake}
        >
          <Power className="h-4 w-4 mr-1" />
          Wake up
        </Button>
      </AlertDescription>
    </Alert>
  );
}
