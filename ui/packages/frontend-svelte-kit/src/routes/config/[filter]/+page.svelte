<script lang="ts">
  import { onMount } from "svelte";
  import { Card, CardContent } from "$lib/components/ui/card";
  import Button from '$lib/components/ui/button/button.svelte';
  import { Input } from "$lib/components/ui/input";
  import { Switch } from "$lib/components/ui/switch";
  import { Label } from "$lib/components/ui/label";
  import * as Select from "$lib/components/ui/select";
  import {
    Alert,
    AlertDescription,
    AlertTitle,
  } from "$lib/components/ui/alert";
  import {
    Loader2,
    Save,
    RefreshCw,
    AlertCircle,
    HelpCircle,
    TriangleAlert,
    ChevronUp,
    ChevronDown,
  } from "lucide-svelte";
  import { toast } from "svelte-sonner";
  import parameterLabels from "$lib/parameter-labels";
  import { parameterHelpTexts } from "$lib/parameter-help-texts";
  import {
    areRequiredParametersMet,
    getMissingRequiredParametersMessage,
  } from "$lib/parameter-utils";
  import {
    Popover,
    PopoverTrigger,
    PopoverContent,
  } from "$lib/components/ui/popover";
  import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
  } from "$lib/components/ui/collapsible";
  import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogFooter,
  } from "$lib/components/ui/dialog";
  import {
    isParameterBoolean,
    isParameterEnum,
    isParameterString,
    ParameterTypes,
    type Parameter,
    type UpdateParameter,
  } from "$lib/types/parameters";

  import { page } from "$app/state";
  import {
    cleverCoffeeState,
    fetchParameters,
    saveParameters,
    updateParameter,
  } from "$lib/stores/clever-coffee-store.svelte";
  import { groups2, mappedParameterGroups } from "$lib/parameter-groups";
  import ParameterNavigation from "$lib/components/ParameterNavigation.svelte";

  let isHardwareWarningOpen = $state(false);
  let originalParameters = $state<Parameter[]>([]);
  let showChangesDialog = $state(false);
  let pendingChanges = $state<
    {
      name: string;
      oldValue: string | number | boolean;
      newValue: string | number | boolean;
    }[]
  >([]);
  let saving = $state(false);

  onMount(() => {
    if (cleverCoffeeState.parameters.length && !originalParameters.length) {
      originalParameters = cleverCoffeeState.parameters.map((p) => ({ ...p }));
    }
  });

  const filter = $derived(page.params.filter);

  const groupedParameters = $derived.by(() => {
    if (!filter) {
      return {};
    }

    return groups2[filter].reduce<Record<string, Parameter[]>>(
      (prev, section) => {
        const tmp: Parameter[] = [];
        const group = mappedParameterGroups.get(section);

        if (!group) {
          return prev;
        }

        group.parameters.forEach((paramName) => {
          const param = cleverCoffeeState.parameters.find(
            (p) => p.name === paramName
          );
          if (param) {
            tmp.push(param);
          }
        });

        prev[group.label] = tmp;
        return prev;
      },
      {}
    );
  });

  const getChangedParameters = () => {
    return cleverCoffeeState.parameters
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

  const updateCompleteParameterValue = (
    paramName: string,
    newValue: string | number | boolean
  ) => {
    updateParameter(paramName, newValue);
  };

  const handleSubmitParameters = async (e: Event) => {
    e.preventDefault();
    const changes = getChangedParameters();
    pendingChanges = changes;
    showChangesDialog = true;
  };

  const handleConfirmSave = async () => {
    saving = true;
    const changedParams: UpdateParameter[] = pendingChanges.map((change) => ({
      name: change.name,
      value: change.newValue,
    }));

    try {
      const success = await saveParameters(changedParams);
      saving = false;
      showChangesDialog = false;

      if (success) {
        toast.success("Parameters saved successfully", {
          description: `Saved ${pendingChanges.length} changed parameters. Settings will take effect after restart.`,
        });
        await fetchParameters();
        originalParameters = cleverCoffeeState.parameters.map((p) => ({
          ...p,
        }));
      } else {
        toast.error("Failed to save parameters", {
          description: "Please check your connection and try again.",
        });
      }
    } catch (err) {
      saving = false;
      showChangesDialog = false;
      toast.error("Failed to save parameters", {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  };

  function handleResetAllChanges() {
    originalParameters.forEach((orig) => {
      const current = cleverCoffeeState.parameters.find(
        (p) => p.name === orig.name
      );
      if (current && current.value !== orig.value) {
        updateParameter(orig.name, orig.value);
      }
    });
    toast.info("All changes have been reset.");
  }
</script>

{#if cleverCoffeeState.loadingParams && !cleverCoffeeState.parameters.length}
  <div class="flex h-[60vh] flex-col items-center justify-center">
    <div class="flex flex-col items-center gap-4">
      <Loader2 class="h-10 w-10 animate-spin text-primary" />
      <span class="text-lg text-muted-foreground">Loading parameters...</span>
    </div>
  </div>
{:else if cleverCoffeeState.errorParams}
  <div
    class="flex flex-col items-center gap-6 rounded-xl border border-destructive/40 bg-destructive/10 p-8 shadow-lg"
  >
    <AlertCircle class="mb-2 h-10 w-10 text-destructive" />
    <h2 class="text-xl font-semibold text-destructive">
      Failed to load parameters
    </h2>
    <p class="max-w-md text-center text-base text-destructive/80">
      The configuration parameters could not be loaded.
      <br />
      Please check your connection and try again.
    </p>
    <Button onclick={() => fetchParameters()} variant="destructive" size="lg">
      <RefreshCw class="mr-2 h-5 w-5" />
      Retry
    </Button>
  </div>
{:else}
  <div class="container mx-auto max-w-7xl space-y-6 p-6">
    <!-- Connection Error Alert -->
    {#if cleverCoffeeState.errorParams}
      <Alert
        class="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive"
      >
        <AlertCircle class="h-4 w-4" />
        <AlertDescription>
          {cleverCoffeeState.errorParams}
          <Button onclick={() => fetchParameters()} class="ml-4">
            <RefreshCw class="mr-2 h-4 w-4" />
            Retry
          </Button>
        </AlertDescription>
      </Alert>
    {/if}

    <!-- Parameter Navigation and Actions -->
    <div class="mb-4 flex items-center justify-between">
      <ParameterNavigation />
      <div class="flex items-center gap-2">
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={cleverCoffeeState.loadingParams ||
            cleverCoffeeState.parameters.length === 0}
          onclick={handleResetAllChanges}
        >
          <RefreshCw class="mr-2 h-4 w-4" />
          Reset All Changes
        </Button>
        <Button
          variant="outline"
          size="sm"
          onclick={() => fetchParameters()}
          disabled={cleverCoffeeState.loadingParams}
          class="ml-2"
        >
          {#if cleverCoffeeState.loadingParams}
            <Loader2 class="mr-2 h-4 w-4 animate-spin" />
          {:else}
            <RefreshCw class="mr-2 h-4 w-4" />
          {/if}
          Refresh Parameters
        </Button>
      </div>
    </div>

    <!-- Hardware Warning -->
    {#if filter === "hardware" && !cleverCoffeeState.loadingParams}
      <Alert variant="destructive">
        <TriangleAlert class="h-4 w-4" />
        <AlertTitle>Hardware Configuration Warning</AlertTitle>
        <AlertDescription>
          <p class="mb-3">
            <strong
              >Incorrect hardware settings can cause dangerous behavior!</strong
            >
          </p>
          <Collapsible bind:open={isHardwareWarningOpen}>
            <CollapsibleContent
              class="data-[state=open]:animate-collapsible-down data-[state=closed]:animate-collapsible-up overflow-hidden"
            >
              <div class="mt-3 space-y-2 text-sm">
                <ul class="list-disc pl-5 space-y-1">
                  <li>
                    <strong>Wrong relay or switch configurations</strong>
                    could cause the pump, heater, or valve to activate unexpectedly
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
                  your settings, only enable features if you understand their functionality
                  and hardware-related prerequisites
                </p>
                <p class="mb-0">
                  <strong>When in doubt:</strong> Read or re-read the
                  documentation or consult the community via our
                  <a
                    href="https://discord.gg/Kq5RFznuU4"
                    target="_blank"
                    rel="noopener noreferrer"
                    class="underline hover:no-underline"
                  >
                    Discord server
                  </a>
                  .
                </p>
              </div>
            </CollapsibleContent>
            <CollapsibleTrigger>
              <Button variant="outline" size="sm" class="mt-3">
                {isHardwareWarningOpen ? "Hide Details" : "Show Details"}
                {#if isHardwareWarningOpen}
                  <ChevronUp class="ml-2 h-4 w-4" />
                {:else}
                  <ChevronDown class="ml-2 h-4 w-4" />
                {/if}
              </Button>
            </CollapsibleTrigger>
          </Collapsible>
        </AlertDescription>
      </Alert>
    {/if}

    <form onsubmit={handleSubmitParameters} class="space-y-6">
      {#each Object.entries(groupedParameters) as [sectionName, sectionParams]}
        {#if sectionParams.length > 0}
          <Card class="mb-8">
            <CardContent>
              <h2 class="mb-4 text-xl font-bold">{sectionName}</h2>
              <div
                class="grid auto-rows-fr grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3"
              >
                {#each sectionParams as param}
                  {@const isDisabled = !areRequiredParametersMet(
                    param,
                    cleverCoffeeState.parameters
                  )}
                  {@const disabledHint = isDisabled
                    ? getMissingRequiredParametersMessage(
                        param,
                        cleverCoffeeState.parameters
                      )
                    : undefined}

                  <div
                    class="flex min-h-[120px] flex-col space-y-3 rounded-lg border bg-card p-4 transition-colors duration-200 hover:bg-accent/50"
                  >
                    <div class="flex items-center justify-between">
                      <Label for={param.name} class="text-sm font-medium">
                        {parameterLabels.en[param.name] || param.name}
                      </Label>
                      {#if parameterHelpTexts[param.name]}
                        <Popover>
                          <PopoverTrigger>
                            <Button
                              variant="ghost"
                              size="icon"
                              class="ml-2 h-6 w-6"
                            >
                              <HelpCircle class="h-4 w-4" />
                            </Button>
                          </PopoverTrigger>
                          <PopoverContent class="w-80 text-xs">
                            {@html parameterHelpTexts[param.name]}
                          </PopoverContent>
                        </Popover>
                      {/if}
                    </div>

                    {#if isParameterBoolean(param)}
                      {#if isDisabled}
                        <Popover>
                          <PopoverTrigger>
                            <div
                              class="flex cursor-help items-center justify-center py-4"
                            >
                              <div
                                class="flex items-center gap-2 rounded-md border border-blue-200 bg-blue-50 px-3 py-2 text-blue-600 dark:border-blue-800 dark:bg-blue-950 dark:text-blue-400"
                              >
                                <HelpCircle class="h-4 w-4" />
                                <span class="text-sm font-medium">Disabled</span
                                >
                              </div>
                            </div>
                          </PopoverTrigger>
                          <PopoverContent class="w-64 text-xs">
                            <div class="space-y-2">
                              <div class="font-medium">Parameter Disabled</div>
                              <div class="text-muted-foreground">
                                {disabledHint}
                              </div>
                            </div>
                          </PopoverContent>
                        </Popover>
                      {:else}
                        <div class="flex items-center space-x-2">
                          <Switch
                            id={param.name}
                            checked={!!param.value}
                            onCheckedChange={(checked) =>
                              updateCompleteParameterValue(
                                param.name,
                                checked ? 1 : 0
                              )}
                          />
                          <span>{param.value ? "On" : "Off"}</span>
                        </div>
                      {/if}
                    {:else if isParameterEnum(param)}
                      {#if isDisabled}
                        <Popover>
                          <PopoverTrigger>
                            <div
                              class="flex cursor-help items-center justify-center py-4"
                            >
                              <div
                                class="flex items-center gap-2 rounded-md border border-blue-200 bg-blue-50 px-3 py-2 text-blue-600 dark:border-blue-800 dark:bg-blue-950 dark:text-blue-400"
                              >
                                <HelpCircle class="h-4 w-4" />
                                <span class="text-sm font-medium">Disabled</span
                                >
                              </div>
                            </div>
                          </PopoverTrigger>
                          <PopoverContent class="w-64 text-xs">
                            <div class="space-y-2">
                              <div class="font-medium">Parameter Disabled</div>
                              <div class="text-muted-foreground">
                                {disabledHint}
                              </div>
                            </div>
                          </PopoverContent>
                        </Popover>
                      {:else}
                        <Select.Root
                          type="single"
                          value={String(param.value)}
                          onValueChange={(value) =>
                            updateCompleteParameterValue(
                              param.name,
                              Number(value)
                            )}
                        >
                          <Select.Trigger class="w-[180px]"></Select.Trigger>
                          <Select.Content>
                            {#each param.options || [] as option}
                              <Select.Item value={String(option.value)}>
                                {option.label}
                              </Select.Item>
                            {/each}
                          </Select.Content>
                        </Select.Root>
                      {/if}
                    {:else if isParameterString(param)}
                      {#if isDisabled}
                        <Popover>
                          <PopoverTrigger>
                            <div
                              class="flex cursor-help items-center justify-center py-4"
                            >
                              <div
                                class="flex items-center gap-2 rounded-md border border-blue-200 bg-blue-50 px-3 py-2 text-blue-600 dark:border-blue-800 dark:bg-blue-950 dark:text-blue-400"
                              >
                                <HelpCircle class="h-4 w-4" />
                                <span class="text-sm font-medium">Disabled</span
                                >
                              </div>
                            </div>
                          </PopoverTrigger>
                          <PopoverContent class="w-64 text-xs">
                            <div class="space-y-2">
                              <div class="font-medium">Parameter Disabled</div>
                              <div class="text-muted-foreground">
                                {disabledHint}
                              </div>
                            </div>
                          </PopoverContent>
                        </Popover>
                      {:else}
                        <Input
                          id={param.name}
                          type={param.name.toLowerCase().includes("password")
                            ? "password"
                            : "text"}
                          value={String(param.value || "")}
                          onchange={(e) =>
                            updateCompleteParameterValue(
                              param.name,
                              (e.target as HTMLInputElement).value
                            )}
                          maxlength={param.max && param.max > 0
                            ? param.max
                            : 64}
                          placeholder={`Enter ${param.name}`}
                        />
                      {/if}
                    {:else if isDisabled}
                      <Popover>
                        <PopoverTrigger>
                          <div
                            class="flex cursor-help items-center justify-center py-4"
                          >
                            <div
                              class="flex items-center gap-2 rounded-md border border-blue-200 bg-blue-50 px-3 py-2 text-blue-600 dark:border-blue-800 dark:bg-blue-950 dark:text-blue-400"
                            >
                              <HelpCircle class="h-4 w-4" />
                              <span class="text-sm font-medium">Disabled</span>
                            </div>
                          </div>
                        </PopoverTrigger>
                        <PopoverContent class="w-64 text-xs">
                          <div class="space-y-2">
                            <div class="font-medium">Parameter Disabled</div>
                            <div class="text-muted-foreground">
                              {disabledHint}
                            </div>
                          </div>
                        </PopoverContent>
                      </Popover>
                    {:else}
                      <Input
                        id={param.name}
                        type="number"
                        step={param.type === ParameterTypes.FLOAT ||
                        param.type === ParameterTypes.DOUBLE
                          ? "0.01"
                          : "1"}
                        min={param.min}
                        max={param.max}
                        value={String(param.value)}
                        onchange={(e) =>
                          updateCompleteParameterValue(
                            param.name,
                            param.type === ParameterTypes.FLOAT ||
                              param.type === ParameterTypes.DOUBLE
                              ? parseFloat((e.target as HTMLInputElement).value)
                              : parseInt(
                                  (e.target as HTMLInputElement).value,
                                  10
                                )
                          )}
                        placeholder={`${param.min} - ${param.max}`}
                      />
                    {/if}
                  </div>
                {/each}
              </div>
            </CardContent>
          </Card>
        {/if}
      {/each}

      {#if cleverCoffeeState.parameters.length > 0}
        <Card>
          <CardContent class="pt-6">
            <div class="flex items-center justify-between">
              <div class="text-sm text-muted-foreground">
                {cleverCoffeeState.parameters.length} parameters loaded
              </div>
              <Button
                type="submit"
                disabled={cleverCoffeeState.loadingParams}
                size="lg"
                class="min-w-[140px]"
              >
                {#if cleverCoffeeState.loadingParams}
                  <Loader2 class="mr-2 h-4 w-4 animate-spin" />
                {:else}
                  <Save class="mr-2 h-4 w-4" />
                {/if}
                {cleverCoffeeState.loadingParams
                  ? "Saving..."
                  : "Save Parameters"}
              </Button>
            </div>
          </CardContent>
        </Card>
      {/if}
    </form>

    <!-- Changes Dialog -->
    <Dialog bind:open={showChangesDialog}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Confirm Parameter Changes</DialogTitle>
        </DialogHeader>
        {#if pendingChanges.length === 0}
          <div class="text-muted-foreground">No changes detected.</div>
        {:else}
          <div class="space-y-2">
            <div class="mb-2 font-medium">
              The following parameters will be updated:
            </div>
            <ul class="list-disc pl-5">
              {#each pendingChanges as change}
                {@const param = cleverCoffeeState.parameters.find(
                  (p) => p.name === change.name
                )}
                {@const label = parameterLabels.en[change.name] || change.name}
                {@const oldValueLabel =
                  param?.options?.find((opt) => opt.value === change.oldValue)
                    ?.label ?? String(change.oldValue)}
                {@const newValueLabel =
                  param?.options?.find((opt) => opt.value === change.newValue)
                    ?.label ?? String(change.newValue)}

                <li>
                  <span class="font-semibold">{label}</span>:
                  <span class="text-muted-foreground">{oldValueLabel}</span> →
                  <span class="font-semibold text-primary">{newValueLabel}</span
                  >
                </li>
              {/each}
            </ul>
          </div>
        {/if}
        <DialogFooter>
          <Button
            variant="outline"
            onclick={() => (showChangesDialog = false)}
            disabled={saving}
          >
            Cancel
          </Button>
          <Button
            onclick={handleConfirmSave}
            disabled={saving || pendingChanges.length === 0}
          >
            {#if saving}
              <Loader2 class="mr-2 h-4 w-4 animate-spin" />
            {:else}
              <Save class="mr-2 h-4 w-4" />
            {/if}
            {saving ? "Saving..." : `Save ${pendingChanges.length} Changes`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
{/if}
