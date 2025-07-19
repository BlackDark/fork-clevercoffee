import { Link, useParams } from "react-router-dom";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Button } from "@/components/ui/button";
import { ChevronDown, Settings } from "lucide-react";

const parameterCategories = [
  {
    filter: "behavior",
    label: "Behavior & Control",
    description: "PID, brewing, temperature, and control settings",
  },
  {
    filter: "hardware",
    label: "Hardware Configuration",
    description: "Sensors, relays, display, and hardware settings",
  },
  {
    filter: "system",
    label: "System & Connectivity",
    description: "MQTT, power management, and system configuration",
  },
];

export function ParameterNavigation() {
  const { filter } = useParams<{ filter: string }>();
  const currentCategory = parameterCategories.find(
    (cat) => cat.filter === filter
  );

  return (
    <div className="flex items-center gap-2 mb-6">
      <Settings className="h-5 w-5 text-muted-foreground" />
      <span className="text-sm font-medium text-muted-foreground">
        Parameter Category:
      </span>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="outline" className="justify-between min-w-[200px]">
            {currentCategory?.label || "Select Category"}
            <ChevronDown className="ml-2 h-4 w-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-80">
          {parameterCategories.map((category) => (
            <DropdownMenuItem key={category.filter} asChild>
              <Link
                to={`/config/${category.filter}`}
                className="flex flex-col items-start p-3 cursor-pointer"
              >
                <div className="font-medium">{category.label}</div>
                <div className="text-xs text-muted-foreground">
                  {category.description}
                </div>
              </Link>
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
