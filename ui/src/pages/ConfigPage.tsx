import { useState, useEffect, useMemo } from "react";
import { useParams } from "react-router-dom";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  ChevronDown,
  ChevronUp,
  TriangleAlert,
  Loader2,
  CheckCircle,
  HelpCircle,
  Save,
  RefreshCw,
  AlertCircle,
} from "lucide-react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@radix-ui/react-collapsible";
import { toast } from "sonner";
import { useCleverCoffee, type Parameter } from "@/hooks/use-clever-coffee";
import { ParameterNavigation } from "@/components/ParameterNavigation";

const sectionNames: Record<number, string> = {
  0: "PID Parameters",
  1: "Temperature",
  2: "Brew PID Parameters",
  3: "Brew Control",
  4: "Scale Parameters",
  5: "Display Settings",
  6: "Maintenance",
  7: "Power Settings",
  8: "MQTT Settings",
  9: "System Settings",
  10: "Other",
  11: "OLED Display",
  12: "Relays",
  13: "Switches",
  14: "LEDs",
  15: "Sensors",
};

export function ConfigPage() {
  const { filter } = useParams<{ filter: string }>();
  const [isHardwareWarningOpen, setIsHardwareWarningOpen] = useState(false);

  // Use the centralized hook for all data and actions
  const {
    // State
    parameters,
    parametersHelpTexts,
    isLoadingParams,
    isPostingForm,
    showPostSucceeded,
    connectionError,

    // Actions
    fetchParameters,
    fetchHelpText,
    updateParameterValue,
    postParameters,
  } = useCleverCoffee();

  // Fetch parameters when filter changes
  useEffect(() => {
    fetchParameters(filter || "");
  }, [fetchParameters, filter]);

  // Helper functions
  const getInputType = (param: Parameter) => {
    switch (param.type) {
      case 5: // enum
        return "select";
      case 4: // string
        return "text";
      case 0: // integer
      case 1: // uint8
      case 2: // double
      case 3: // float
        return "number";
      default:
        return "text";
    }
  };

  const getNumberStep = (param: Parameter) => {
    switch (param.type) {
      case 0: // integer
      case 1: // uint8
        return 1;
      case 2: // double
      case 3: // float
        return 0.01;
      default:
        return 1;
    }
  };

  const isBoolean = (param: Parameter) => {
    return param.type === 1 && param.min === 0 && param.max === 1;
  };

  // Handle form submission
  const handleSubmitParameters = async (e: React.FormEvent) => {
    e.preventDefault();

    const parameterNames = parameters.map((param) => param.name);
    const success = await postParameters(parameterNames);

    if (success) {
      toast.success("Parameters saved successfully", {
        description: `Saved ${parameters.length} parameters. Settings will take effect after restart.`,
      });
      // Re-fetch to get updated values/conditions
      fetchParameters(filter || "");
    } else {
      toast.error("Failed to save parameters", {
        description: "Please check your connection and try again.",
      });
    }
  };

  // Group parameters by section for display
  const groupedParameters = useMemo(() => {
    const result: Record<string, Parameter[]> = {};
    parameters.forEach((param) => {
      const section = Math.floor(param.position / 100);
      const sectionName = sectionNames[section] || `Section ${section}`;
      if (!result[sectionName]) {
        result[sectionName] = [];
      }
      result[sectionName].push(param);
    });
    return result;
  }, [parameters]);

  return (
    <div className="container mx-auto p-6 space-y-6 max-w-7xl">
      {/* Connection Error Alert */}
      {connectionError && (
        <Alert className="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription className="flex items-center justify-between">
            <span>{connectionError}</span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                fetchParameters(filter || "");
              }}
              className="ml-3"
            >
              <RefreshCw className="h-4 w-4 mr-1" />
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {/* Parameter Navigation */}
      {!isLoadingParams && <ParameterNavigation />}

      {/* Loading State */}
      {isLoadingParams && (
        <Card>
          <CardContent className="flex items-center justify-center py-12">
            <div className="flex flex-col items-center space-y-3">
              <Loader2 className="h-8 w-8 animate-spin text-primary" />
              <p className="text-sm text-muted-foreground">
                Loading parameters...
              </p>
            </div>
          </CardContent>
        </Card>
      )}

      {/* Hardware Warning */}
      {filter === "hardware" && !isLoadingParams && (
        <Alert variant="destructive">
          <TriangleAlert className="h-4 w-4" />
          <AlertTitle>Hardware Configuration Warning</AlertTitle>
          <AlertDescription>
            <p className="mb-3">
              <strong>
                Incorrect hardware settings can cause dangerous behavior!
              </strong>
            </p>
            <Collapsible
              open={isHardwareWarningOpen}
              onOpenChange={setIsHardwareWarningOpen}
            >
              <CollapsibleContent className="data-[state=open]:animate-collapsible-down data-[state=closed]:animate-collapsible-up overflow-hidden">
                <div className="text-sm space-y-2 mt-3">
                  <ul className="list-disc pl-5 space-y-1">
                    <li>
                      <strong>Wrong relay or switch configurations</strong>{" "}
                      could cause the pump, heater, or valve to activate
                      unexpectedly
                    </li>
                    <li>
                      <strong>Incorrect sensor configuration</strong> may
                      prevent the machine from starting up normally
                    </li>
                    <li>
                      Using the <strong>wrong OLED type or i2c address</strong>{" "}
                      may lead to graphics errors or the display not working at
                      all
                    </li>
                  </ul>
                  <p>
                    <strong>Before saving and restarting:</strong> Double-check
                    all your settings, only enable features if you understand
                    their functionality and hardware-related prerequisites
                  </p>
                  <p className="mb-0">
                    <strong>When in doubt:</strong> Read or re-read the
                    documentation or consult the community via our{" "}
                    <a
                      href="https://discord.gg/Kq5RFznuU4"
                      target="_blank"
                      rel="noopener noreferrer"
                      className="underline hover:no-underline"
                    >
                      Discord server
                    </a>
                    .
                  </p>
                </div>
              </CollapsibleContent>
              <CollapsibleTrigger asChild>
                <Button variant="outline" size="sm" className="mt-3">
                  {isHardwareWarningOpen ? "Hide Details" : "Show Details"}
                  {isHardwareWarningOpen ? (
                    <ChevronUp className="ml-2 h-4 w-4" />
                  ) : (
                    <ChevronDown className="ml-2 h-4 w-4" />
                  )}
                </Button>
              </CollapsibleTrigger>
            </Collapsible>
          </AlertDescription>
        </Alert>
      )}

      {/* Parameters Form */}
      {!isLoadingParams && (
        <form onSubmit={handleSubmitParameters} className="space-y-6">
          {parameters.length === 0 ? (
            <Card>
              <CardContent className="flex flex-col items-center justify-center py-12">
                <AlertCircle className="h-12 w-12 text-muted-foreground mb-4" />
                <h3 className="text-lg font-semibold mb-2">
                  No Parameters Found
                </h3>
                <p className="text-muted-foreground text-center max-w-md">
                  No parameters are available for the selected filter. Try
                  selecting a different category or check your connection.
                </p>
                <Button
                  variant="outline"
                  className="mt-4"
                  onClick={() => fetchParameters(filter || "")}
                  disabled={isLoadingParams}
                >
                  <RefreshCw className="mr-2 h-4 w-4" />
                  Refresh
                </Button>
              </CardContent>
            </Card>
          ) : (
            Object.entries(groupedParameters).map(
              ([sectionName, sectionParams], sectionIndex) => (
                <Card key={sectionName}>
                  <CardHeader>
                    <CardTitle className="flex items-center gap-3">
                      <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10">
                        <span className="text-sm font-bold text-primary">
                          {sectionIndex + 1}
                        </span>
                      </div>
                      {sectionName}
                      <span className="ml-auto text-sm font-normal text-muted-foreground">
                        {sectionParams.length} parameters
                      </span>
                    </CardTitle>
                  </CardHeader>
                  <CardContent className="space-y-6">
                    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                      {sectionParams.map((param) => (
                        <div
                          key={param.name}
                          className="space-y-3 p-4 rounded-lg border bg-card hover:bg-accent/50 transition-colors duration-200"
                        >
                          <div className="flex items-center justify-between">
                            <Label
                              htmlFor={param.name}
                              className="text-sm font-medium"
                            >
                              {param.displayName}
                            </Label>
                            {param.hasHelpText && (
                              <Popover>
                                <PopoverTrigger asChild>
                                  <Button
                                    variant="ghost"
                                    size="icon"
                                    className="h-6 w-6"
                                    onMouseEnter={() =>
                                      fetchHelpText(param.name)
                                    }
                                  >
                                    <HelpCircle className="h-4 w-4" />
                                  </Button>
                                </PopoverTrigger>
                                <PopoverContent className="w-80">
                                  <div className="space-y-2">
                                    <h4 className="font-medium">
                                      {param.displayName}
                                    </h4>
                                    <p className="text-sm text-muted-foreground">
                                      {parametersHelpTexts[param.name] || (
                                        <span className="flex items-center">
                                          <Loader2 className="mr-2 h-3 w-3 animate-spin" />
                                          Loading help text...
                                        </span>
                                      )}
                                    </p>
                                  </div>
                                </PopoverContent>
                              </Popover>
                            )}
                          </div>

                          {isBoolean(param) ? (
                            <div className="flex items-center space-x-3">
                              <Switch
                                id={param.name}
                                checked={param.value !== 0}
                                onCheckedChange={(checked) =>
                                  updateParameterValue(
                                    param.name,
                                    checked ? 1 : 0
                                  )
                                }
                                className="data-[state=checked]:bg-green-600"
                              />
                              <Label
                                htmlFor={param.name}
                                className="text-sm text-muted-foreground"
                              >
                                {param.value !== 0 ? "Enabled" : "Disabled"}
                              </Label>
                            </div>
                          ) : getInputType(param) === "text" ? (
                            <Input
                              type={
                                param.name.toLowerCase().includes("password")
                                  ? "password"
                                  : "text"
                              }
                              id={param.name}
                              value={String(param.value || "")}
                              onChange={(e) =>
                                updateParameterValue(param.name, e.target.value)
                              }
                              maxLength={param.max > 0 ? param.max : 64}
                              placeholder={`Enter ${param.displayName.toLowerCase()}`}
                            />
                          ) : (
                            <Input
                              type="number"
                              step={getNumberStep(param)}
                              min={param.min}
                              max={param.max}
                              id={param.name}
                              value={String(param.value)}
                              onChange={(e) =>
                                updateParameterValue(
                                  param.name,
                                  parseFloat(e.target.value) || 0
                                )
                              }
                              placeholder={`${param.min} - ${param.max}`}
                            />
                          )}

                          {/* Validation Messages */}
                          {param.type !== 4 &&
                            typeof param.value === "number" &&
                            (param.value < param.min ||
                              param.value > param.max) && (
                              <div className="flex items-center space-x-2 text-destructive text-xs">
                                <AlertCircle className="h-3 w-3" />
                                <span>
                                  Value must be between {param.min} and{" "}
                                  {param.max}
                                </span>
                              </div>
                            )}
                          {param.type === 4 &&
                            param.max > 0 &&
                            typeof param.value === "string" &&
                            param.value.length > param.max && (
                              <div className="flex items-center space-x-2 text-destructive text-xs">
                                <AlertCircle className="h-3 w-3" />
                                <span>
                                  Text too long (max {param.max} characters)
                                </span>
                              </div>
                            )}
                        </div>
                      ))}
                    </div>
                  </CardContent>
                </Card>
              )
            )
          )}

          {/* Save Button */}
          {parameters.length > 0 && (
            <Card>
              <CardContent className="pt-6">
                <div className="flex items-center justify-between">
                  <div className="text-sm text-muted-foreground">
                    {parameters.length} parameters loaded
                  </div>
                  <Button
                    type="submit"
                    disabled={isPostingForm}
                    size="lg"
                    className={`min-w-[140px] ${
                      showPostSucceeded ? "bg-green-600 hover:bg-green-700" : ""
                    }`}
                  >
                    {isPostingForm ? (
                      <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    ) : showPostSucceeded ? (
                      <CheckCircle className="mr-2 h-4 w-4" />
                    ) : (
                      <Save className="mr-2 h-4 w-4" />
                    )}
                    {isPostingForm
                      ? "Saving..."
                      : showPostSucceeded
                      ? "Saved Successfully!"
                      : "Save Parameters"}
                  </Button>
                </div>
              </CardContent>
            </Card>
          )}
        </form>
      )}
    </div>
  );
}
