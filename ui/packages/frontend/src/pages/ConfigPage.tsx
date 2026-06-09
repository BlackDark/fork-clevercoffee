import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@radix-ui/react-collapsible";
import {
  AlertCircle,
  ChevronDown,
  ChevronUp,
  HelpCircle,
  ListRestart,
  Loader2,
  RefreshCw,
  Save,
  TriangleAlert,
} from "lucide-react";
import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import { toast } from "sonner";
import { MaintenanceBackflushPanel } from "@/components/MaintenanceBackflushPanel";
import { ParameterNavigation } from "@/components/ParameterNavigation";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { useCleverCoffee } from "@/context/useCleverCoffee";
import { groups2, mappedParameterGroups } from "@/lib";
import { apiFetch } from "@/lib/api-config";
import { parameterHelpTexts } from "@/lib/parameter-help-texts";
import parameterLabels from "@/lib/parameter-labels";
import { ensureCompleteParameters } from "@/lib/parameter-metadata";
import {
  getRebootParameterLabels,
  parameterRequiresReboot,
} from "@/lib/parameter-reboot-required";
import type { Parameter, UpdateParameter } from "@/lib/parameter-types";
import {
  isParameterBoolean,
  isParameterEnum,
  isParameterString,
  ParameterTypes,
} from "@/lib/parameter-types";
import {
  areRequiredParametersMet,
  getMissingRequiredParametersMessage,
} from "@/lib/parameter-utils";
import { API_ROUTES } from "@/lib/routes";

// Extract types for better type safety
interface ParameterChange {
  name: string;
  oldValue: string | number | boolean;
  newValue: string | number | boolean;
}

// Extract sub-components for better organization and performance
const LoadingState = () => (
  <div className="flex flex-col items-center justify-center h-[60vh]">
    <div className="flex flex-col items-center gap-4">
      <Loader2 className="h-10 w-10 animate-spin text-primary" />
      <span className="text-lg text-muted-foreground">
        Loading parameters...
      </span>
    </div>
  </div>
);

const ErrorState = ({
  error,
  onRetry,
}: {
  error: string;
  onRetry: () => void;
}) => (
  <div className="flex flex-col items-center gap-6 p-8 rounded-xl border border-destructive/40 bg-destructive/10 shadow-lg">
    <AlertCircle className="h-10 w-10 text-destructive mb-2" />
    <h2 className="text-xl font-semibold text-destructive">
      Failed to load parameters
    </h2>
    <p className="text-base text-destructive/80 text-center max-w-md">
      The configuration parameters could not be loaded.
      <br />
      Please check your connection and try again.
      <br />
      {error}
    </p>
    <Button onClick={onRetry} variant="destructive" size="lg">
      <RefreshCw className="mr-2 h-5 w-5" />
      Retry
    </Button>
  </div>
);

const HardwareWarning = ({
  isOpen,
  onToggle,
}: {
  isOpen: boolean;
  onToggle: (open: boolean) => void;
}) => (
  <Alert variant="destructive" className="mt-4">
    <TriangleAlert className="h-4 w-4" />
    <AlertTitle>Hardware Configuration Warning</AlertTitle>
    <AlertDescription>
      <Collapsible open={isOpen} onOpenChange={onToggle}>
        <CollapsibleContent className="data-[state=open]:animate-collapsible-down data-[state=closed]:animate-collapsible-up overflow-hidden">
          <div className="text-sm space-y-2 mt-3 max-h-64 overflow-y-auto pr-2">
            <p className="mb-3">
              <strong>
                Incorrect hardware settings can cause dangerous behavior!
              </strong>
            </p>
            <ul className="list-disc pl-5 space-y-1">
              <li>
                <strong>Wrong relay or switch configurations</strong> could
                cause the pump, heater, or valve to activate unexpectedly
              </li>
              <li>
                <strong>Incorrect sensor configuration</strong> may prevent the
                machine from starting up normally
              </li>
              <li>
                Using the <strong>wrong OLED type or i2c address</strong> may
                lead to graphics errors or the display not working at all
              </li>
            </ul>
            <p>
              <strong>Before saving and restarting:</strong> Double-check all
              your settings, only enable features if you understand their
              functionality and hardware-related prerequisites
            </p>
            <p className="mb-0">
              <strong>When in doubt:</strong> Read or re-read the documentation
              or consult the community via our{" "}
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
            {isOpen ? "Hide Details" : "Show Details"}
            {isOpen ? (
              <ChevronUp className="ml-2 h-4 w-4" />
            ) : (
              <ChevronDown className="ml-2 h-4 w-4" />
            )}
          </Button>
        </CollapsibleTrigger>
      </Collapsible>
    </AlertDescription>
  </Alert>
);

const RebootRequiredBanner = ({
  labels,
  onReboot,
  onDismiss,
}: {
  labels: string[];
  onReboot: () => void;
  onDismiss: () => void;
}) => (
  <Alert className="mb-4 border-blue-200 bg-blue-50 text-blue-950 dark:border-blue-900 dark:bg-blue-950/40 dark:text-blue-100">
    <ListRestart className="h-4 w-4" />
    <AlertTitle>Reboot required</AlertTitle>
    <AlertDescription>
      <p className="mb-3 text-sm">
        The following settings require a reboot to take effect:{" "}
        <em>{labels.join(", ")}</em>
      </p>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" onClick={onReboot}>
          <ListRestart className="mr-2 h-4 w-4" />
          Reboot Now
        </Button>
        <Button size="sm" variant="outline" onClick={onDismiss}>
          Later
        </Button>
      </div>
    </AlertDescription>
  </Alert>
);

const DisabledParameterState = ({
  disabledHint,
}: {
  disabledHint?: string;
}) => (
  <Popover>
    <PopoverTrigger asChild>
      <div className="flex items-center justify-center cursor-help">
        <div className="flex items-center gap-2 px-3 py-2 bg-blue-50 dark:bg-blue-950 text-blue-600 dark:text-blue-400 rounded-md border border-blue-200 dark:border-blue-800">
          <HelpCircle className="h-4 w-4" />
          <span className="text-sm font-medium">Disabled</span>
        </div>
      </div>
    </PopoverTrigger>
    <PopoverContent className="w-64 text-xs">
      <div className="space-y-2">
        <div className="font-medium">Parameter Disabled</div>
        <div className="text-muted-foreground">{disabledHint}</div>
      </div>
    </PopoverContent>
  </Popover>
);

// Memoized parameter input component
const ParameterInput = React.memo(
  ({
    param,
    isDisabled,
    disabledHint,
    onUpdate,
  }: {
    param: Parameter;
    isDisabled: boolean;
    disabledHint?: string;
    onUpdate: (value: string | number | boolean) => void;
  }) => {
    if (isDisabled) {
      return <DisabledParameterState disabledHint={disabledHint} />;
    }

    if (isParameterBoolean(param)) {
      return (
        <div className="flex items-center space-x-2">
          <Switch
            id={param.name}
            checked={!!param.value}
            onCheckedChange={(checked) => onUpdate(checked)}
          />
          <span>{param.value ? "On" : "Off"}</span>
        </div>
      );
    }

    if (isParameterEnum(param)) {
      return (
        <Select
          value={String(param.value)}
          onValueChange={(value) => onUpdate(Number(value))}
        >
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {param.options?.map((option) => (
              <SelectItem key={option.value} value={String(option.value)}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      );
    }

    if (isParameterString(param)) {
      return (
        <Input
          id={param.name}
          type={
            param.name.toLowerCase().includes("password") ? "password" : "text"
          }
          value={String(param.value || "")}
          onChange={(e) => onUpdate(e.target.value)}
          maxLength={param.max && param.max > 0 ? param.max : 64}
          placeholder={`Enter ${param.name}`}
        />
      );
    }

    // Numeric input
    return (
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
          onUpdate(
            param.type === ParameterTypes.FLOAT ||
              param.type === ParameterTypes.DOUBLE
              ? parseFloat(e.target.value)
              : parseInt(e.target.value, 10),
          )
        }
        placeholder={`${param.min} - ${param.max}`}
      />
    );
  },
);

export function ConfigPage() {
  const { filter } = useParams<{ filter: string }>();
  const [isHardwareWarningOpen, setIsHardwareWarningOpen] = useState(false);
  const [editedParameters, setEditedParameters] = useState<Parameter[] | null>(
    null,
  );
  const [showChangesDialog, setShowChangesDialog] = useState(false);
  const [saving, setSaving] = useState(false);
  const [showRebootBanner, setShowRebootBanner] = useState(false);
  const [changedRebootParamLabels, setChangedRebootParamLabels] = useState<
    string[]
  >([]);

  const {
    parameters: serverParameters,
    saveParameters,
    loadingParams,
    fetchParameters,
    errorParams,
  } = useCleverCoffee();

  const [prevServerParameters, setPrevServerParameters] =
    useState(serverParameters);

  if (serverParameters !== prevServerParameters) {
    setPrevServerParameters(serverParameters);
    setEditedParameters(null);
  }

  const baseParameters = useMemo(
    () =>
      serverParameters.length > 0
        ? ensureCompleteParameters(serverParameters).map((parameter) => ({
            ...parameter,
          }))
        : [],
    [serverParameters],
  );

  const localParameters = editedParameters ?? baseParameters;

  useEffect(() => {
    if (serverParameters.length === 0) fetchParameters();
  }, [fetchParameters, serverParameters.length]);

  // Simple parameter update - just update local state
  const updateLocalParameter = useCallback(
    (paramName: string, newValue: string | number | boolean) => {
      setEditedParameters((previous) => {
        const current = previous ?? baseParameters;
        return current.map((param) =>
          param.name === paramName ? { ...param, value: newValue } : param,
        );
      });
    },
    [baseParameters],
  );

  // Reset all changes to server state
  const resetAllChanges = useCallback(() => {
    setEditedParameters(null);
  }, []);

  // Calculate changes between local and server state
  const changedParameters = useMemo((): ParameterChange[] => {
    if (serverParameters.length === 0 || localParameters.length === 0)
      return [];

    const serverMap = new Map(serverParameters.map((p) => [p.name, p]));

    return localParameters
      .filter((localParam) => {
        const serverParam = serverMap.get(localParam.name);
        // Only include parameters that exist on server and have changed values
        return serverParam && localParam.value !== serverParam.value;
      })
      .map((localParam) => {
        const serverParam = serverMap.get(localParam.name)!;
        return {
          name: localParam.name,
          oldValue: serverParam.value as string | number | boolean,
          newValue: localParam.value as string | number | boolean,
        };
      });
  }, [localParameters, serverParameters]);

  // Grouped parameters computation - simple version
  const groupedParameters = useMemo(() => {
    if (!filter || !groups2[filter] || localParameters.length === 0) {
      return {};
    }

    const parameterMap = new Map(localParameters.map((p) => [p.name, p]));
    const groups: Record<string, Parameter[]> = {};

    groups2[filter].forEach((section) => {
      const group = mappedParameterGroups.get(section);
      if (!group) return;

      const sectionParams = group.parameters
        .map((paramName) => parameterMap.get(paramName))
        .filter((param): param is Parameter => param !== undefined);

      if (sectionParams.length > 0) {
        groups[group.label] = sectionParams;
      }
    });

    return groups;
  }, [localParameters, filter]);

  const handleSubmitParameters = useCallback((e: React.FormEvent) => {
    e.preventDefault();
    setShowChangesDialog(true);
  }, []);

  const handleConfirmSave = useCallback(async () => {
    if (changedParameters.length === 0) return;

    setSaving(true);

    const changedParams: UpdateParameter[] = changedParameters.map(
      (change) => ({
        name: change.name,
        value: change.newValue,
      }),
    );

    const rebootParamNames = changedParameters
      .map((change) => change.name)
      .filter(parameterRequiresReboot);

    try {
      const success = await saveParameters(changedParams);

      if (success) {
        if (rebootParamNames.length > 0) {
          setChangedRebootParamLabels(
            getRebootParameterLabels(rebootParamNames),
          );
          setShowRebootBanner(true);
          toast.success("Parameters saved successfully", {
            description: `${rebootParamNames.length} changed setting(s) require a reboot.`,
          });
        } else {
          toast.success("Parameters saved successfully");
        }

        // Refresh parameters from server
        await fetchParameters();
      } else {
        toast.error("Failed to save parameters", {
          description: "Please check your connection and try again.",
        });
      }
    } catch (err) {
      toast.error("Failed to save parameters", {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setSaving(false);
      setShowChangesDialog(false);
    }
  }, [changedParameters, saveParameters, fetchParameters]);

  const handleRebootNow = useCallback(async () => {
    setShowRebootBanner(false);
    setChangedRebootParamLabels([]);
    try {
      await apiFetch(API_ROUTES.RESTART, { method: "POST" });
      toast.success("Restart initiated", {
        description: "Machine is restarting...",
      });
    } catch (error: unknown) {
      console.log("Machine restarting...", error);
      toast.success("Restart initiated", {
        description: "Machine is restarting...",
      });
    }
  }, []);

  const dismissRebootBanner = useCallback(() => {
    setShowRebootBanner(false);
    setChangedRebootParamLabels([]);
  }, []);

  const hasParameters = localParameters.length > 0;
  const isHardwareFilter = filter === "hardware";
  const hasChanges = changedParameters.length > 0;

  return (
    <div className="fixed inset-0 top-16 flex flex-col">
      {/* Header Section - Always visible (flex-shrink-0) */}
      <div className="flex-shrink-0 bg-background border-b shadow-sm">
        <div className="container mx-auto px-6 pt-6 pb-4 max-w-7xl">
          {/* Connection Error Alert */}
          {errorParams && (
            <Alert className="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive mb-4">
              <AlertCircle className="h-4 w-4" />
              <AlertDescription>
                {errorParams}
                <Button onClick={() => fetchParameters()} className="ml-4">
                  <RefreshCw className="mr-2 h-4 w-4" />
                  Retry
                </Button>
              </AlertDescription>
            </Alert>
          )}

          {showRebootBanner && changedRebootParamLabels.length > 0 && (
            <RebootRequiredBanner
              labels={changedRebootParamLabels}
              onReboot={handleRebootNow}
              onDismiss={dismissRebootBanner}
            />
          )}

          {/* Parameter Navigation and Actions */}
          <div className="flex flex-col space-y-3 md:flex-row md:items-center md:justify-between md:space-y-0 mb-4">
            <div className="mb-2 md:mb-0">
              <ParameterNavigation />
            </div>
            <div className="flex flex-col space-y-2 sm:flex-row sm:space-y-0 sm:space-x-2">
              <Button
                type="submit"
                disabled={loadingParams || !hasChanges}
                size="sm"
                className="min-w-[120px] w-full sm:w-auto"
                onClick={handleSubmitParameters}
              >
                {loadingParams ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Save className="mr-2 h-4 w-4" />
                )}
                {loadingParams ? "Saving..." : "Save Parameters"}
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="sm"
                disabled={loadingParams || !hasParameters || !hasChanges}
                onClick={resetAllChanges}
                className="w-full sm:w-auto"
              >
                <ListRestart className="mr-2 h-4 w-4" />
                Reset Changes
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => fetchParameters()}
                disabled={loadingParams}
                className="w-full sm:w-auto"
              >
                {loadingParams ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <RefreshCw className="mr-2 h-4 w-4" />
                )}
                Refresh Parameters
              </Button>
            </div>
          </div>

          {/* Parameter Count and Changes Info */}
          {hasParameters && (
            <div className="text-sm text-muted-foreground">
              {localParameters.length} parameters
              {hasChanges && ` • ${changedParameters.length} changes`}
            </div>
          )}

          {/* Hardware Warning */}
          {isHardwareFilter && !loadingParams && (
            <HardwareWarning
              isOpen={isHardwareWarningOpen}
              onToggle={setIsHardwareWarningOpen}
            />
          )}
        </div>
      </div>

      {/* Main Content Area - Scrollable (flex-1) */}
      <div className="flex-1 overflow-auto min-h-0">
        <div className="container mx-auto px-6 py-6 max-w-7xl">
          {/* Show loading state */}
          {loadingParams && !localParameters.length && <LoadingState />}

          {/* Show error state */}
          {errorParams && !loadingParams && (
            <ErrorState error={errorParams} onRetry={fetchParameters} />
          )}

          {/* Show parameters form */}
          {hasParameters && (
            <form onSubmit={handleSubmitParameters} className="space-y-6">
              {Object.entries(groupedParameters).map(
                ([sectionName, sectionParams]) => (
                  <Card key={sectionName} className="mb-8">
                    <CardContent>
                      <h2 className="text-xl font-bold mb-4">{sectionName}</h2>
                      {sectionName === "Maintenance" && (
                        <MaintenanceBackflushPanel />
                      )}
                      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 auto-rows-fr">
                        {sectionParams.map((param) => {
                          // Simple inline requirement check - no complex memoization
                          const isDisabled = !areRequiredParametersMet(
                            param,
                            localParameters,
                          );
                          const disabledHint = isDisabled
                            ? getMissingRequiredParametersMessage(
                                param,
                                localParameters,
                              )
                            : undefined;

                          return (
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
                                          __html:
                                            parameterHelpTexts[param.name],
                                        }}
                                      />
                                    </PopoverContent>
                                  </Popover>
                                )}
                              </div>

                              <ParameterInput
                                param={param}
                                isDisabled={isDisabled}
                                disabledHint={disabledHint}
                                onUpdate={(value) =>
                                  updateLocalParameter(param.name, value)
                                }
                              />
                            </div>
                          );
                        })}
                      </div>
                    </CardContent>
                  </Card>
                ),
              )}
            </form>
          )}
        </div>
      </div>

      {/* Changes Dialog */}
      <Dialog open={showChangesDialog} onOpenChange={setShowChangesDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Confirm Parameter Changes</DialogTitle>
          </DialogHeader>
          {changedParameters.length === 0 ? (
            <div className="text-muted-foreground">No changes detected.</div>
          ) : (
            <div className="space-y-2">
              <div className="font-medium mb-2">
                The following parameters will be updated:
              </div>
              <ul className="list-disc pl-5 max-h-64 overflow-y-auto">
                {changedParameters.map((change) => {
                  const param = localParameters.find(
                    (p) => p.name === change.name,
                  );
                  const label = parameterLabels.en[change.name] || change.name;

                  let oldValueLabel = String(change.oldValue);
                  let newValueLabel = String(change.newValue);

                  if (param?.options) {
                    const oldOpt = param.options.find(
                      (opt) => opt.value === change.oldValue,
                    );
                    const newOpt = param.options.find(
                      (opt) => opt.value === change.newValue,
                    );
                    if (oldOpt) oldValueLabel = oldOpt.label;
                    if (newOpt) newValueLabel = newOpt.label;
                  }

                  return (
                    <li key={change.name}>
                      <span className="font-semibold">{label}</span>:{" "}
                      <span className="text-muted-foreground">
                        {oldValueLabel}
                      </span>{" "}
                      →{" "}
                      <span className="text-primary font-semibold">
                        {newValueLabel}
                      </span>
                    </li>
                  );
                })}
              </ul>
            </div>
          )}
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setShowChangesDialog(false)}
              disabled={saving}
            >
              Cancel
            </Button>
            <Button
              onClick={handleConfirmSave}
              disabled={saving || changedParameters.length === 0}
            >
              {saving ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Save className="mr-2 h-4 w-4" />
              )}
              {saving
                ? "Saving..."
                : `Save ${changedParameters.length} Changes`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
