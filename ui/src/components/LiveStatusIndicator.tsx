import { useCleverCoffee } from "@/hooks/use-clever-coffee";
import { Wifi, WifiOff, Clock } from "lucide-react";

export function LiveStatusIndicator() {
  const { isOnline, lastHealthCheck } = useCleverCoffee();

  const getLastCheckText = () => {
    if (!lastHealthCheck) return "Never";

    const now = new Date();
    const diff = now.getTime() - lastHealthCheck.getTime();
    const seconds = Math.floor(diff / 1000);

    if (seconds < 5) return "Just now";
    if (seconds < 60) return `${seconds}s ago`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
    return `${Math.floor(seconds / 3600)}h ago`;
  };

  return (
    <div className="flex items-center gap-2 text-sm">
      <div className="flex items-center gap-1">
        {isOnline ? (
          <>
            <Wifi className="h-4 w-4 text-green-600" />
            <span className="text-green-600 font-medium">Online</span>
            <div className="h-2 w-2 bg-green-500 rounded-full animate-pulse" />
          </>
        ) : (
          <>
            <WifiOff className="h-4 w-4 text-red-600" />
            <span className="text-red-600 font-medium">Offline</span>
            <div className="h-2 w-2 bg-red-500 rounded-full" />
          </>
        )}
      </div>

      {lastHealthCheck && (
        <div className="flex items-center gap-1 text-muted-foreground">
          <Clock className="h-3 w-3" />
          <span className="text-xs">{getLastCheckText()}</span>
        </div>
      )}
    </div>
  );
}
