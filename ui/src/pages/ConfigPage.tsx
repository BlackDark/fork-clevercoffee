import { useState, useMemo } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import {
  Loader2,
  CheckCircle,
  Save,
  RefreshCw,
  AlertCircle,
  HelpCircle,
  TriangleAlert,
  ChevronUp,
  ChevronDown,
} from "lucide-react";
import { toast } from "sonner";
import { ParameterTypes, isParameterBoolean } from "@/lib/parameter-types";
import type { Parameter as OldParameter } from "@/types/parameters";
import type { Parameter as NewParameter } from "@/lib/parameter-types";
import { useCleverCoffee } from "@/hooks/use-clever-coffee";
import parameterLabels from "@/lib/parameter-labels";
import { parameterGroups } from "@/lib/parameter-groups";
import { parameterHelpTexts } from "@/lib/parameter-help-texts";
import { mergeParametersWithDefaults } from "@/lib/parameter-definitions";
import { shouldShowParameter } from "@/lib/parameter-utils";
import {
  Popover,
  PopoverTrigger,
  PopoverContent,
} from "@/components/ui/popover";
import { ParameterNavigation } from "@/components/ParameterNavigation";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@radix-ui/react-collapsible";
import { useParams } from "react-router-dom";

// Helper function to convert from old Parameter type to new Parameter type
const convertParameterType = (oldParam: OldParameter): NewParameter => {
  return {
    type: oldParam.type,
    name: oldParam.name,
    displayName: oldParam.name, // Use name as displayName if not provided
    section: oldParam.section || 0,
    position: oldParam.position || 0,
    hasHelpText: false,
    show: true,
    value: oldParam.value as string | number | boolean,
    min: oldParam.min || 0,
    max: oldParam.max || 100,
    options: oldParam.options,
  };
};

export function ConfigPage() {
  const { filter } = useParams<{ filter: string }>();
  const [isHardwareWarningOpen, setIsHardwareWarningOpen] = useState(false);

  // Use the existing clever coffee hook
  const {
    parameters,
    isLoadingParams,
    isPostingForm,
    showPostSucceeded,
    connectionError,
    fetchParameters,
    updateParameterValue,
    postParameters,
  } = useCleverCoffee();

  // Map parameter group keys to categories
  const groupCategoryMap = useMemo(
    (): Record<string, string> => ({
      pidParameters: "behavior",
      temperatureControl: "behavior",
      brewPidSection: "behavior",
      brewControl: "behavior",
      scaleParameters: "behavior",
      displaySettings: "behavior",
      maintenance: "behavior",
      powerSettings: "behavior",
      mqttSettings: "system",
      systemSettings: "system",
      systemAuth: "system",
      runtimeControls: "system",
      oledDisplay: "hardware",
      relays: "hardware",
      switchesBrew: "hardware",
      switchesSteam: "hardware",
      switchesPower: "hardware",
      switchesHotWater: "hardware",
      ledsStatus: "hardware",
      ledsBrew: "hardware",
      ledsSteam: "hardware",
      sensorTemperature: "hardware",
      sensorPressure: "hardware",
      sensorWatertank: "hardware",
      sensorScale: "hardware",
    }),
    []
  );

  // State to track local parameter changes
  const [localParameterChanges, setLocalParameterChanges] = useState<
    Record<string, unknown>
  >({});

  // Merge server parameters with complete definitions to ensure ALL parameters are available
  const completeParameters = useMemo(() => {
    const convertedParameters = parameters.map(convertParameterType);
    const merged = mergeParametersWithDefaults(convertedParameters);
    // Apply local changes with proper typing
    return merged.map(
      (param): NewParameter => ({
        ...param,
        value:
          (localParameterChanges[param.name] as string | number | boolean) ??
          param.value,
      })
    );
  }, [parameters, localParameterChanges]);

  // Enhanced parameter visibility logic using complete parameters
  const visibleParameters = useMemo(() => {
    return completeParameters.filter((param) =>
      shouldShowParameter(param, completeParameters)
    );
  }, [completeParameters]);

  // Custom update function that handles both server and local parameters
  const updateCompleteParameterValue = (
    paramName: string,
    newValue: unknown
  ) => {
    // Update local changes
    setLocalParameterChanges((prev) => ({
      ...prev,
      [paramName]: newValue,
    }));

    // Also update the server parameters if they exist (for compatibility)
    updateParameterValue(paramName, newValue);
  };

  // Group parameters for display
  const groupedParameters = useMemo(() => {
    const selectedCategory = filter || "behavior";
    const filteredGroups = parameterGroups.filter(
      (group) => groupCategoryMap[group.key] === selectedCategory
    );

    const paramMap = Object.fromEntries(
      visibleParameters.map((p) => [p.name, p])
    );

    const result: Record<string, typeof visibleParameters> = {};
    filteredGroups.forEach((group) => {
      const groupParams = group.parameters
        .map((name) => paramMap[name])
        .filter(Boolean);

      if (groupParams.length > 0) {
        result[group.label] = groupParams;
      }
    });

    return result;
  }, [visibleParameters, filter, groupCategoryMap]);

  // Handle form submission
  const handleSubmitParameters = async (e: React.FormEvent) => {
    e.preventDefault();
    const parameterNames = visibleParameters.map((param) => param.name);
    const success = await postParameters(parameterNames);
    if (success) {
      toast.success("Parameters saved successfully", {
        description: `Saved ${visibleParameters.length} parameters. Settings will take effect after restart.`,
      });
      fetchParameters();
    } else {
      toast.error("Failed to save parameters", {
        description: "Please check your connection and try again.",
      });
    }
  };

  if (
    (isLoadingParams && !parameters.length) ||
    (!isLoadingParams && parameters.length === 0)
  ) {
    return (
      <div className="flex flex-col items-center justify-center h-[60vh]">
        {isLoadingParams ? (
          <div className="flex flex-col items-center gap-4">
            <Loader2 className="h-10 w-10 animate-spin text-primary" />
            <span className="text-lg text-muted-foreground">
              Loading parameters...
            </span>
          </div>
        ) : (
          <div className="flex flex-col items-center gap-6 p-8 rounded-xl border border-destructive/40 bg-destructive/10 shadow-lg">
            <AlertCircle className="h-10 w-10 text-destructive mb-2" />
            <h2 className="text-xl font-semibold text-destructive">
              Failed to load parameters
            </h2>
            <p className="text-base text-destructive/80 text-center max-w-md">
              The configuration parameters could not be loaded.
              <br />
              Please check your connection and try again.
            </p>
            <Button
              onClick={() => fetchParameters()}
              variant="destructive"
              size="lg"
            >
              <RefreshCw className="mr-2 h-5 w-5" />
              Retry
            </Button>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="container mx-auto p-6 space-y-6 max-w-7xl">
      {/* Connection Error Alert */}
      {connectionError && (
        <Alert className="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            {connectionError}
            <Button onClick={() => fetchParameters()} className="ml-4">
              <RefreshCw className="mr-2 h-4 w-4" />
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {/* Parameter Navigation */}
      <div className="flex items-center justify-between mb-4">
        <ParameterNavigation />
        <Button
          variant="outline"
          size="sm"
          onClick={() => fetchParameters()}
          disabled={isLoadingParams}
          className="ml-2"
        >
          {isLoadingParams ? (
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          ) : (
            <RefreshCw className="mr-2 h-4 w-4" />
          )}
          Refresh Parameters
        </Button>
      </div>

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

      <form onSubmit={handleSubmitParameters} className="space-y-6">
        {Object.entries(groupedParameters).map(
          ([sectionName, sectionParams]) =>
            sectionParams.length > 0 && (
              <Card key={sectionName} className="mb-8">
                <CardContent>
                  <h2 className="text-xl font-bold mb-4">{sectionName}</h2>
                  <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 auto-rows-fr">
                    {sectionParams
                      .sort((a, b) => (a.position ?? 0) - (b.position ?? 0))
                      .map((param) => (
                        <div
                          key={param.name}
                          className="flex flex-col space-y-3 p-4 rounded-lg border bg-card hover:bg-accent/50 transition-colors duration-200 min-h-[120px]"
                        >
                          <div className="flex items-center justify-between">
                            <Label
                              htmlFor={param.name}
                              className="text-sm font-medium"
                            >
                              {parameterLabels.en[param.name] || param.name}
                            </Label>
                            {parameterHelpTexts[param.name] && (
                              <Popover>
                                <PopoverTrigger asChild>
                                  <Button
                                    variant="ghost"
                                    size="icon"
                                    className="h-6 w-6 ml-2"
                                    tabIndex={0}
                                  >
                                    <HelpCircle className="h-4 w-4" />
                                  </Button>
                                </PopoverTrigger>
                                <PopoverContent className="w-80 text-xs">
                                  <span
                                    dangerouslySetInnerHTML={{
                                      __html: parameterHelpTexts[param.name],
                                    }}
                                  />
                                </PopoverContent>
                              </Popover>
                            )}
                          </div>

                          {isParameterBoolean(param) ? (
                            <div className="flex items-center space-x-2">
                              <Switch
                                id={param.name}
                                checked={!!param.value}
                                onCheckedChange={(checked) =>
                                  updateCompleteParameterValue(
                                    param.name,
                                    checked ? 1 : 0
                                  )
                                }
                              />
                              <span>{param.value ? "On" : "Off"}</span>
                            </div>
                          ) : param.type === ParameterTypes.ENUM ? (
                            <Select
                              value={String(param.value)}
                              onValueChange={(value) =>
                                updateCompleteParameterValue(
                                  param.name,
                                  Number(value)
                                )
                              }
                            >
                              <SelectTrigger>
                                <SelectValue />
                              </SelectTrigger>
                              <SelectContent>
                                {param.options?.map((option) => (
                                  <SelectItem
                                    key={option.value}
                                    value={String(option.value)}
                                  >
                                    {option.label}
                                  </SelectItem>
                                ))}
                              </SelectContent>
                            </Select>
                          ) : param.type === ParameterTypes.STRING ? (
                            <Input
                              id={param.name}
                              type={
                                param.name.toLowerCase().includes("password")
                                  ? "password"
                                  : "text"
                              }
                              value={String(param.value || "")}
                              onChange={(e) =>
                                updateCompleteParameterValue(
                                  param.name,
                                  e.target.value
                                )
                              }
                              maxLength={
                                param.max && param.max > 0 ? param.max : 64
                              }
                              placeholder={`Enter ${param.name}`}
                            />
                          ) : (
                            <Input
                              id={param.name}
                              type="number"
                              step={
                                param.type === ParameterTypes.FLOAT ||
                                param.type === ParameterTypes.DOUBLE
                                  ? "0.01"
                                  : "1"
                              }
                              min={param.min}
                              max={param.max}
                              value={String(param.value)}
                              onChange={(e) =>
                                updateCompleteParameterValue(
                                  param.name,
                                  param.type === ParameterTypes.FLOAT ||
                                    param.type === ParameterTypes.DOUBLE
                                    ? parseFloat(e.target.value)
                                    : parseInt(e.target.value, 10)
                                )
                              }
                              placeholder={`${param.min} - ${param.max}`}
                            />
                          )}
                        </div>
                      ))}
                  </div>
                </CardContent>
              </Card>
            )
        )}
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
    </div>
  );
}
