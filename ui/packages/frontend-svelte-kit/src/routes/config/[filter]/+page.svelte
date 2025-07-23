<script lang="ts">
  import { page } from '$app/stores';
  import { onMount, onDestroy } from 'svelte';
  import { Card, CardContent } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Switch } from '$lib/components/ui/switch';
  import { Label } from '$lib/components/ui/label';
  import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
  } from '$lib/components/ui/select';
  import { Alert, AlertDescription, AlertTitle } from '$lib/components/ui/alert';
  import {
    Loader2,
    Save,
    RefreshCw,
    AlertCircle,
    HelpCircle,
    TriangleAlert,
    ChevronUp,
    ChevronDown,
  } from 'lucide-svelte';
  import { toast } from 'svelte-sonner';
  import { ParameterTypes, isParameterBoolean, isParameterEnum, isParameterString } from '$lib/types/parameters';
  import parameterLabels from '$lib/parameter-labels';
  import { parameterHelpTexts } from '$lib/parameter-help-texts';
  import { areRequiredParametersMet, getMissingRequiredParametersMessage } from '$lib/parameter-utils';
  import {
    Popover,
    PopoverTrigger,
    PopoverContent,
  } from '$lib/components/ui/popover';
  import ParameterNavigation from '$lib/components/ParameterNavigation.svelte';
  import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
  } from '$lib/components/ui/collapsible';
  import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '$lib/components/ui/dialog';
  import type { Parameter } from '$lib/types/parameters';
  import { useEnhancedParameters } from '$lib/hooks/use-enhanced-parameters';

  $: filter = $page.params.filter;
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

  // Use the enhanced clever coffee hook
  const {
    parameters,
    visibleParameters,
    groupedParameters,
    loading,
    error,
    updateParameter,
    saveParameters,
    fetchParameters,
  } = useEnhancedParameters(filter);

  // On initial load, store original parameters
  onMount(() => {
    if (parameters.length && !originalParameters.length) {
      originalParameters = parameters.map((p) => ({ ...p }));
    }
  });

  // Reactively update originalParameters when parameters change after initial load
  $effect(() => {
    if (parameters.length && originalParameters.length === 0) {
      originalParameters = parameters.map((p) => ({ ...p }));
    }
  });

  // Compute changed parameters
  function getChangedParameters() {
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
  }

  // Custom update function for parameters
  function updateCompleteParameterValue(
    paramName: string,
    newValue: string | number | boolean
  ) {
    updateParameter(paramName, newValue);
  }

  // Handle form submission
  function handleSubmitParameters() {
    const changes = getChangedParameters();
    pendingChanges = changes;
    showChangesDialog = true;
  }

  // Confirm save (only changed parameters)
  async function handleConfirmSave() {
    saving = true;
    // Collect changed parameters as a map
    const changedParams: Record<string, string | number | boolean> = {};
    pendingChanges.forEach((change) => {
      changedParams[change.name] = change.newValue;
    });
    // Pass map to saveParameters
    try {
      const success = await saveParameters(changedParams);
      saving = false;
      showChangesDialog = false;
      if (success) {
        toast.success("Parameters saved successfully", {
          description: `Saved ${pendingChanges.length} changed parameters. Settings will take effect after restart.`,
        });
        await fetchParameters();
        originalParameters = parameters.map((p) => ({ ...p }));
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
  }

  // Reset all changes
  function resetAllChanges() {
    originalParameters.forEach((orig) => {
      const current = parameters.find((p) => p.name === orig.name);
      if (current && current.value !== orig.value) {
        updateParameter(orig.name, orig.value);
      }
    });
    toast.info("All changes have been reset.");
  }

  // Update filter when route changes
  $effect(() => {
    filter = $page.params.filter;
  });
</script>

{#if loading && parameters.length === 0}
  <div class="flex flex-col items-center justify-center h-[60vh]">
    <div class="flex flex-col items-center gap-4">
      <Loader2 class="h-10 w-10 animate-spin text-primary" />
      <span class="text-lg text-muted-foreground">
        Loading parameters...
      </span>
    </div>
  </div>
{:else if error}
  <div class="flex flex-col items-center gap-6 p-8 rounded-xl border border-destructive/40 bg-destructive/10 shadow-lg">
    <AlertCircle class="h-10 w-10 text-destructive mb-2" />
    <h2 class="text-xl font-semibold text-destructive">
      Failed to load parameters
    </h2>
    <p class="text-base text-destructive/80 text-center max-w-md">
      The configuration parameters could not be loaded.
      <br />
      Please check your connection and try again.
    </p>
    <Button
      on:click={() => fetchParameters()}
      variant="destructive"
      size="lg"
    >
      <RefreshCw class="mr-2 h-5 w-5" />
      Retry
    </Button>
  </div>
{:else}
  <div class="container mx-auto p-6 space-y-6 max-w-7xl">
    <!-- Connection Error Alert -->
    {#if error}
      <Alert class="border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive">
        <AlertCircle class="h-4 w-4" />
        <AlertDescription>
          {error}
          <Button on:click={() => fetchParameters()} class="ml-4">
            <RefreshCw class="mr-2 h-4 w-4" />
            Retry
          </Button>
        </AlertDescription>
      </Alert>
    {/if}

    <!-- Parameter Navigation and Actions -->
    <div class="flex items-center justify-between mb-4">
      <ParameterNavigation />
      <div class="flex items-center gap-2">
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={loading || parameters.length === 0}
          on:click={resetAllChanges}
        >
          <RefreshCw class="mr-2 h-4 w-4" />
          Reset All Changes
        </Button>
        <Button
          variant="outline"
          size="sm"
          on:click={() => fetchParameters()}
          disabled={loading}
          class="ml-2"
        >
          {#if loading}
            <Loader2 class="mr-2 h-4 w-4 animate-spin" />
          {:else}
            <RefreshCw class="mr-2 h-4 w-4" />
          {/if}
          Refresh Parameters
        </Button>
      </div>
    </div>

    <!-- Hardware Warning -->
    {#if filter === "hardware" && !loading}
      <Alert variant="destructive">
        <TriangleAlert class="h-4 w-4" />
        <AlertTitle>Hardware Configuration Warning</AlertTitle>
        <AlertDescription>
          <p class="mb-3">
            <strong>
              Incorrect hardware settings can cause dangerous behavior!
            </strong>
          </p>
          <Collapsible
            bind:open={isHardwareWarningOpen}
          >
            <CollapsibleContent class="data-[state=open]:animate-collapsible-down data-[state=closed]:animate-collapsible-up overflow-hidden">
              <div class="text-sm space-y-2 mt-3">
                <ul class="list-disc pl-5 space-y-1">
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
                <p class="mb-0">
                  <strong>When in doubt:</strong> Read or re-read the
                  documentation or consult the community via our{" "}
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
            <CollapsibleTrigger asChild>
              <Button variant="outline" size="sm" class="mt-3">
                {#if isHardwareWarningOpen}
                  Hide Details
                {:else}
                  Show Details
                {/if}
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

    <form onsubmit|preventDefault={handleSubmitParameters} class="space-y-6">
      {#each Object.entries(groupedParameters) as [sectionName, sectionParams]}
        {#if sectionParams.length > 0}
          <Card key={sectionName} class="mb-8">
            <CardContent>
              <h2 class="text-xl font-bold mb-4">{sectionName}</h2>
              <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 auto-rows-fr">
                {#each sectionParams as param}
                  {@const isDisabled = !areRequiredParametersMet(param, visibleParameters)}
                  {@const disabledHint = isDisabled
                    ? getMissingRequiredParametersMessage(param, visibleParameters)
                    : undefined}

                  <div
                    key={param.name}
                    class="flex flex-col space-y-3 p-4 rounded-lg border bg-card hover:bg-accent/50 transition-colors duration-200 min-h-[120px]"
                  >
                    <div class="flex items-center justify-between">
                      <Label
                        for={param.name}
                        class="text-sm font-medium"
                      >
                        {parameterLabels.en[param.name] || param.name}
                      </Label>
                      {#if parameterHelpTexts[param.name]}
                        <Popover>
                          <PopoverTrigger asChild>
                            <Button
                              variant="ghost"
                              size="icon"
                              class="h-6 w-6 ml-2"
                              tabindex="0"
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
                          <PopoverTrigger asChild>
                            <div class="flex items-center justify-center py-4 cursor-help">
                              <div class="flex items-center gap-2 px-3 py-2 bg-blue-50 dark:bg-blue-950 text-blue-600 dark:text-blue-400 rounded-md border border-blue-200 dark:border-blue-800">
                                <HelpCircle class="h-4 w-4" />
                                <span class="text-sm font-medium">
                                  Disabled
                                </span>
                              </div>
                            </div>
                          </PopoverTrigger>
                          <PopoverContent class="w-64 text-xs">
                            <div class="space-y-2">
                              <div class="font-medium">
                                Parameter Disabled
                              </div>
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
                              )
                            }
                          />
                          <span>{param.value ? "On" : "Off"}</span>
                        </div>
                      {/if}
                    {:else if isParameterEnum(param)}
                      {#if isDisabled}
                        <Popover>
                          <PopoverTrigger asChild>
                            <div class="flex items-center justify-center py-4 cursor-help">
                              <div class="flex items-center gap-2 px-3 py-2 bg-blue-50 dark:bg-blue-950 text-blue-600 dark:text-blue-400 rounded-md border border-blue-200 dark:border-blue-800">
                                <HelpCircle class="h-4 w-4" />
                                <span class="text-sm font-medium">
                                  Disabled
                                </span>
                              </div>
                            </div>
                          </PopoverTrigger>
                          <PopoverContent class="w-64 text-xs">
                            <div class="space-y-2">
                              <div class="font-medium">
                                Parameter Disabled
                              </div>
                              <div class="text-muted-foreground">
                                {disabledHint}
                              </div>
                            </div>
                          </PopoverContent>
                        </Popover>
                      {:else}
                        <Select
                          selected={{ value: String(param.value), label: param.options?.find(o => o.value === param.value)?.label || String(param.value) }}
                          onSelectedChange={(selected) => {
                            if (selected) {
                              updateCompleteParameterValue(
                                param.name,
                                Number(selected.value)
                              );
                            }
                          }}
                          options={param.options?.map(option => ({ value: String(option.value), label: option.label })) || []}
                        >
                          <SelectTrigger>
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent>
                            {#each param.options || [] as option}
                              <SelectItem
                                value={String(option.value)}
                                label={option.label}
                              >
                                {option.label}
                              </SelectItem>
                            {/each}
                          </SelectContent>
                        </Select>
                      {/if}
                    {:else if isParameterString(param)}
                      {#if isDisabled}
                        <Popover>
                          <PopoverTrigger asChild>
                            <div class="flex items-center justify-center py-4 cursor-help">
                              <div class="flex items-center gap-2 px-3 py-2 bg-blue-50 dark:bg-blue-950 text-blue-600 dark:text-blue-400 rounded-md border border-blue-200 dark:border-blue-800">
                                <HelpCircle class="h-4 w-4" />
                                <span class="text-sm font-medium">
                                  Disabled
                                </span>
                              </div>
                            </div>
                          </PopoverTrigger>
                          <PopoverContent class="w-64 text-xs">
                            <div class="space-y-2">
                              <div class="font-medium">
                                Parameter Disabled
                              </div>
                              <div class="text-muted-foreground">
                                {disabledHint}
                              </div>
                            </div>
                          </PopoverContent>
                        </Popover>
                      {:else}
                        <Input
                          id={param.name}
                          type={param.name.toLowerCase().includes("password") ? "password" : "text"}
                          value={String(param.value || "")}
                          on:input={(e) =>
                            updateCompleteParameterValue(
                              param.name,
                              (e.target as HTMLInputElement).value
                            )
                          }
                          maxlength={param.max && param.max > 0 ? param.max : 64}
                          placeholder={`Enter ${param.name}`}
                        />
                      {/if}
                    {:else if isDisabled}
                      <Popover>
                        <PopoverTrigger asChild>
                          <div class="flex items-center justify-center py-4 cursor-help">
                            <div class="flex items-center gap-2 px-3 py-2 bg-blue-50 dark:bg-blue-950 text-blue-600 dark:text-blue-400 rounded-md border border-blue-200 dark:border-blue-800">
                              <HelpCircle class="h-4 w-4" />
                              <span class="text-sm font-medium">
                                Disabled
                              </span>
                            </div>
                          </div>
                        </PopoverTrigger>
                        <PopoverContent class="w-64 text-xs">
                          <div class="space-y-2">
                            <div class="font-medium">
                              Parameter Disabled
                            </div>
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
                        step={param.type === ParameterTypes.FLOAT || param.type === ParameterTypes.DOUBLE ? "0.01" : "1"}
                        min={param.min}
                        max={param.max}
                        value={String(param.value)}
                        on:input={(e) =>
                          updateCompleteParameterValue(
                            param.name,
                            param.type === ParameterTypes.FLOAT || param.type === ParameterTypes.DOUBLE
                              ? parseFloat((e.target as HTMLInputElement).value)
                              : parseInt((e.target as HTMLInputElement).value, 10)
                          )
                        }
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
      {#if parameters.length > 0}
        <Card>
          <CardContent class="pt-6">
            <div class="flex items-center justify-between">
              <div class="text-sm text-muted-foreground">
                {parameters.length} parameters loaded
              </div>
              <Button
                type="submit"
                disabled={loading}
                size="lg"
                class={`min-w-[140px]`}
              >
                {#if loading}
                  <Loader2 class="mr-2 h-4 w-4 animate-spin" />
                {:else}
                  <Save class="mr-2 h-4 w-4" />
                {/if}
                {#if loading}
                  Saving...
                {:else}
                  Save Parameters
                {/if}
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
            <div class="font-medium mb-2">
              The following parameters will be updated:
            </div>
            <ul class="list-disc pl-5">
              {#each pendingChanges as change}
                {@const param = parameters.find((p) => p.name === change.name)}
                {@const label = parameterLabels.en[change.name] || change.name}
                {@const oldValueLabel = param && param.options ? param.options.find(opt => opt.value === change.oldValue)?.label || String(change.oldValue) : String(change.oldValue)}
                {@const newValueLabel = param && param.options ? param.options.find(opt => opt.value === change.newValue)?.label || String(change.newValue) : String(change.newValue)}
                <li>
                  <span class="font-semibold">{label}</span>:{" "}
                  <span class="text-muted-foreground">
                    {oldValueLabel}
                  </span>{" "}
                  →{" "}
                  <span class="text-primary font-semibold">
                    {newValueLabel}
                  </span>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
        <DialogFooter>
          <Button
            variant="outline"
            on:click={() => (showChangesDialog = false)}
            disabled={saving}
          >
            Cancel
          </Button>
          <Button
            on:click={handleConfirmSave}
            disabled={saving || pendingChanges.length === 0}
          >
            {#if saving}
              <Loader2 class="mr-2 h-4 w-4 animate-spin" />
            {:else}
              <Save class="mr-2 h-4 w-4" />
            {/if}
            {#if saving}
              Saving...
            {:else}
              Save {pendingChanges.length} Changes
            {/if}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
{/if}
