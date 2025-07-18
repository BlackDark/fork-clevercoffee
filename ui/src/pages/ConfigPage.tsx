import { useState, useEffect } from "react";
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
  Save,
  RefreshCw,
  AlertCircle,
  HelpCircle,
  TriangleAlert,
  ChevronUp,
  ChevronDown,
} from "lucide-react";
import { toast } from "sonner";
import { useEnhancedParameters } from "@/hooks/use-enhanced-parameters";
import {
  ParameterTypes,
  isParameterBoolean,
  isParameterEnum,
  isParameterString,
} from "@/lib/parameter-types";
import parameterLabels from "@/lib/parameter-labels";
import { parameterHelpTexts } from "@/lib/parameter-help-texts";
import {
  areRequiredParametersMet,
  getMissingRequiredParametersMessage,
} from "@/lib/parameter-utils";
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
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import type { Parameter } from "@/lib/parameter-types";

export function ConfigPage() {
  const { filter } = useParams<{ filter: string }>();
  const [isHardwareWarningOpen, setIsHardwareWarningOpen] = useState(false);
  const [originalParameters, setOriginalParameters] = useState<Parameter[]>([]);
  const [showChangesDialog, setShowChangesDialog] = useState(false);
  const [pendingChanges, setPendingChanges] = useState<
    {
      name: string;
      oldValue: string | number | boolean;
      newValue: string | number | boolean;
    }[]
  >([]);
  const [saving, setSaving] = useState(false);

  // Use the enhanced clever coffee hook
  const {
    parameters,
    visibleParameters,
    groupedParameters,
    loading,
    error,
    updateParameter,
    saveParameters,
    refreshParameters,
  } = useEnhancedParameters(filter);

  // On initial load, store original parameters
  useEffect(() => {
    if (parameters.length && !originalParameters.length) {
      setOriginalParameters(parameters.map((p) => ({ ...p })));
    }
  }, [parameters, originalParameters.length]);

  // Compute changed parameters
  const getChangedParameters = () => {
    return parameters
      .filter((param) => {
        const orig = originalParameters.find((p) => p.name === param.name);
        return orig && param.value !== orig.value;
      })
      .map((param) => {
        const orig = originalParameters.find((p) => p.name === param.name);
        return {
          name: param.name,
          oldValue: orig?.value as string | number | boolean,
          newValue: param.value as string | number | boolean,
        };
      });
  };

  // Custom update function for parameters
  const updateCompleteParameterValue = (
    paramName: string,
    newValue: string | number | boolean
  ) => {
    updateParameter(paramName, newValue);
  };

  // Handle form submission
  const handleSubmitParameters = async (e: React.FormEvent) => {
    e.preventDefault();
    const changes = getChangedParameters();
    setPendingChanges(changes);
    setShowChangesDialog(true);
  };

  // Confirm save (only changed parameters)
  const handleConfirmSave = async () => {
    setSaving(true);
    // Collect changed parameters as a map
    const changedParams: Record<string, string | number | boolean> = {};
    pendingChanges.forEach((change) => {
      changedParams[change.name] = change.newValue;
    });
    // Pass map to saveParameters
    try {
      const success = await saveParameters(changedParams);
      setSaving(false);
      setShowChangesDialog(false);
      if (success) {
        toast.success("Parameters saved successfully", {
          description: `Saved ${pendingChanges.length} changed parameters. Settings will take effect after restart.`,
        });
        await refreshParameters();
        setOriginalParameters(parameters.map((p) => ({ ...p })));
      } else {
        toast.error("Failed to save parameters", {
          description: "Please check your connection and try again.",
        });
      }
    } catch (err) {
      setSaving(false);
      setShowChangesDialog(false);
      toast.error("Failed to save parameters", {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  };

  if (loading && !parameters.length) {
    return (
      <div className="flex flex-col items-center justify-center h-[60vh]">
        <div className="flex flex-col items-center gap-4">
          <Loader2 className="h-10 w-10 animate-spin text-primary" />
          <span className="text-lg text-muted-foreground">
            Loading parameters...
          </span>
        </div>
      </div>
    );
  }

  if (error) {
    return (
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
          onClick={() => refreshParameters()}
          variant="destructive"
          size="lg"
        >
          <RefreshCw className="mr-2 h-5 w-5" />
          Retry
        </Button>
      </div>
    );
  }

  return (
    <div className="container mx-auto p-6 space-y-6 max-w-7xl">
      {/* Connection Error Alert */}
      {error && (
        <Alert className="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive">
          <AlertCircle className="h-4 w-4" />
          <AlertDescription>
            {error}
            <Button onClick={() => refreshParameters()} className="ml-4">
              <RefreshCw className="mr-2 h-4 w-4" />
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      )}

      {/* Parameter Navigation and Actions */}
      <div className="flex items-center justify-between mb-4">
        <ParameterNavigation />
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            disabled={loading || parameters.length === 0}
            onClick={() => {
              // Reset all parameters to their original values
              originalParameters.forEach((orig) => {
                const current = parameters.find((p) => p.name === orig.name);
                if (current && current.value !== orig.value) {
                  updateParameter(orig.name, orig.value);
                }
              });
              toast.info("All changes have been reset.");
            }}
          >
            <RefreshCw className="mr-2 h-4 w-4" />
            Reset All Changes
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => refreshParameters()}
            disabled={loading}
            className="ml-2"
          >
            {loading ? (
              <Loader2 className="mr-2 h-4 w-4 animate-spin" />
            ) : (
              <RefreshCw className="mr-2 h-4 w-4" />
            )}
            Refresh Parameters
          </Button>
        </div>
      </div>

      {/* Hardware Warning */}
      {filter === "hardware" && !loading && (
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
                    {sectionParams.map((param) => {
                      const isDisabled = !areRequiredParametersMet(
                        param,
                        visibleParameters
                      );
                      const disabledHint = isDisabled
                        ? getMissingRequiredParametersMessage(
                            param,
                            visibleParameters
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
                                      __html: parameterHelpTexts[param.name],
                                    }}
                                  />
                                </PopoverContent>
                              </Popover>
                            )}
                          </div>

                          {isParameterBoolean(param) ? (
                            isDisabled ? (
                              <Popover>
                                <PopoverTrigger asChild>
                                  <div className="flex items-center justify-center py-4 cursor-help">
                                    <div className="flex items-center gap-2 px-3 py-2 bg-blue-50 dark:bg-blue-950 text-blue-600 dark:text-blue-400 rounded-md border border-blue-200 dark:border-blue-800">
                                      <HelpCircle className="h-4 w-4" />
                                      <span className="text-sm font-medium">
                                        Disabled
                                      </span>
                                    </div>
                                  </div>
                                </PopoverTrigger>
                                <PopoverContent className="w-64 text-xs">
                                  <div className="space-y-2">
                                    <div className="font-medium">
                                      Parameter Disabled
                                    </div>
                                    <div className="text-muted-foreground">
                                      {disabledHint}
                                    </div>
                                  </div>
                                </PopoverContent>
                              </Popover>
                            ) : (
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
                            )
                          ) : isParameterEnum(param) ? (
                            isDisabled ? (
                              <Popover>
                                <PopoverTrigger asChild>
                                  <div className="flex items-center justify-center py-4 cursor-help">
                                    <div className="flex items-center gap-2 px-3 py-2 bg-blue-50 dark:bg-blue-950 text-blue-600 dark:text-blue-400 rounded-md border border-blue-200 dark:border-blue-800">
                                      <HelpCircle className="h-4 w-4" />
                                      <span className="text-sm font-medium">
                                        Disabled
                                      </span>
                                    </div>
                                  </div>
                                </PopoverTrigger>
                                <PopoverContent className="w-64 text-xs">
                                  <div className="space-y-2">
                                    <div className="font-medium">
                                      Parameter Disabled
                                    </div>
                                    <div className="text-muted-foreground">
                                      {disabledHint}
                                    </div>
                                  </div>
                                </PopoverContent>
                              </Popover>
                            ) : (
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
                            )
                          ) : isParameterString(param) ? (
                            isDisabled ? (
                              <Popover>
                                <PopoverTrigger asChild>
                                  <div className="flex items-center justify-center py-4 cursor-help">
                                    <div className="flex items-center gap-2 px-3 py-2 bg-blue-50 dark:bg-blue-950 text-blue-600 dark:text-blue-400 rounded-md border border-blue-200 dark:border-blue-800">
                                      <HelpCircle className="h-4 w-4" />
                                      <span className="text-sm font-medium">
                                        Disabled
                                      </span>
                                    </div>
                                  </div>
                                </PopoverTrigger>
                                <PopoverContent className="w-64 text-xs">
                                  <div className="space-y-2">
                                    <div className="font-medium">
                                      Parameter Disabled
                                    </div>
                                    <div className="text-muted-foreground">
                                      {disabledHint}
                                    </div>
                                  </div>
                                </PopoverContent>
                              </Popover>
                            ) : (
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
                            )
                          ) : isDisabled ? (
                            <Popover>
                              <PopoverTrigger asChild>
                                <div className="flex items-center justify-center py-4 cursor-help">
                                  <div className="flex items-center gap-2 px-3 py-2 bg-blue-50 dark:bg-blue-950 text-blue-600 dark:text-blue-400 rounded-md border border-blue-200 dark:border-blue-800">
                                    <HelpCircle className="h-4 w-4" />
                                    <span className="text-sm font-medium">
                                      Disabled
                                    </span>
                                  </div>
                                </div>
                              </PopoverTrigger>
                              <PopoverContent className="w-64 text-xs">
                                <div className="space-y-2">
                                  <div className="font-medium">
                                    Parameter Disabled
                                  </div>
                                  <div className="text-muted-foreground">
                                    {disabledHint}
                                  </div>
                                </div>
                              </PopoverContent>
                            </Popover>
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
                      );
                    })}
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
                  disabled={loading}
                  size="lg"
                  className={`min-w-[140px]`}
                >
                  {loading ? (
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  ) : (
                    <Save className="mr-2 h-4 w-4" />
                  )}
                  {loading ? "Saving..." : "Save Parameters"}
                </Button>
              </div>
            </CardContent>
          </Card>
        )}
      </form>
      {/* Changes Dialog */}
      <Dialog open={showChangesDialog} onOpenChange={setShowChangesDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Confirm Parameter Changes</DialogTitle>
          </DialogHeader>
          {pendingChanges.length === 0 ? (
            <div className="text-muted-foreground">No changes detected.</div>
          ) : (
            <div className="space-y-2">
              <div className="font-medium mb-2">
                The following parameters will be updated:
              </div>
              <ul className="list-disc pl-5">
                {pendingChanges.map((change) => {
                  const param = parameters.find((p) => p.name === change.name);
                  const label = parameterLabels.en[change.name] || change.name;
                  let oldValueLabel = String(change.oldValue);
                  let newValueLabel = String(change.newValue);
                  if (param && param.options) {
                    const oldOpt = param.options.find(
                      (opt) => opt.value === change.oldValue
                    );
                    const newOpt = param.options.find(
                      (opt) => opt.value === change.newValue
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
              disabled={saving || pendingChanges.length === 0}
            >
              {saving ? (
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
              ) : (
                <Save className="mr-2 h-4 w-4" />
              )}
              {saving ? "Saving..." : `Save ${pendingChanges.length} Changes`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
