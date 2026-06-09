import { Wifi, WifiOff } from "lucide-react";
import { useCleverCoffee } from "@/context/useCleverCoffee";

export function LiveStatusIndicator() {
  const { isOnline } = useCleverCoffee();

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
    </div>
  );
}
