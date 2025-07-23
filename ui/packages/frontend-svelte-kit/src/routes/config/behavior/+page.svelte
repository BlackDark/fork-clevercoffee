<script lang="ts">
  import { onMount } from 'svelte';
  import { getParameterLabel } from '$lib/parameter-labels';
  import { basePath } from '$lib/config';
  import {
    parameters,
    loadingParams,
    errorParams,
    fetchParameters,
    updateParameter,
    saveParameters
  } from '$lib/stores/clever-coffee-store.svelte';

  // Filter categories
  const categories = [
    { id: 'behavior', label: 'Behavior' },
    { id: 'hardware', label: 'Hardware' },
    { id: 'system', label: 'System' },
    { id: 'all', label: 'All Parameters' }
  ];

  const activeFilter = 'behavior';

  $: filteredParameters = $parameters.filter(param => {
    // For behavior filter
    if (activeFilter === 'behavior') {
      return param.name.startsWith('brew') ||
             param.name.startsWith('pid') ||
             param.name.startsWith('steam') ||
             param.name.startsWith('backflush') ||
             param.name.startsWith('standby');
    }
    return false;
  });

  onMount(() => {
    fetchParameters();
  });

  async function handleSave() {
    await saveParameters();
  }

  // Event handler functions with proper typing
  function handleCheckboxChange(e: Event, paramName: string) {
    const target = e.target as HTMLInputElement;
    updateParameter(paramName, target.checked ? 1 : 0);
  }

  function handleSelectChange(e: Event, paramName: string) {
    const target = e.target as HTMLSelectElement;
    updateParameter(paramName, Number(target.value));
  }

  function handleTextInput(e: Event, paramName: string) {
    const target = e.target as HTMLInputElement;
    updateParameter(paramName, target.value);
  }

  function handleNumberInput(e: Event, paramName: string) {
    const target = e.target as HTMLInputElement;
    updateParameter(paramName, Number(target.value));
  }
</script>

<div class="container mx-auto p-6">
  <h1 class="text-3xl font-bold mb-6">Configuration</h1>

  <div class="mb-6 flex flex-wrap gap-2">
    {#each categories as category}
      <a
        href={`${basePath}/config/${category.id}`}
        class="px-4 py-2 rounded-md {activeFilter === category.id ? 'bg-primary text-primary-foreground' : 'bg-muted hover:bg-muted/80'}"
      >
        {category.label}
      </a>
    {/each}
  </div>

  {#if $loadingParams}
    <div class="animate-pulse space-y-4">
      {#each Array(5) as _}
        <div class="h-16 bg-muted rounded"></div>
      {/each}
    </div>
  {:else if $errorParams}
    <div class="p-4 bg-red-100 text-red-700 rounded-md">{$errorParams}</div>
  {:else if filteredParameters.length === 0}
    <div class="p-4 bg-muted rounded-md">No parameters found for this filter.</div>
  {:else}
    <div class="flex justify-end mb-4">
      <button
        class="px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90"
        onclick={handleSave}
      >
        Save Changes
      </button>
    </div>

    <div class="space-y-4">
      {#each filteredParameters as param}
        <div class="p-4 bg-card rounded-md border">
          <div class="font-medium">{getParameterLabel(param.name)}</div>
          <div class="text-sm text-muted-foreground">{param.name}</div>
          <div class="mt-2">
            <!-- Simple parameter value display/edit -->
            {#if param.type === 1 && param.min === 0 && param.max === 1}
              <!-- Boolean parameter -->
              <label class="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={param.value === 1 || param.value === true}
                  onchange={(e) => handleCheckboxChange(e, param.name)}
                />
                <span>Enabled</span>
              </label>
            {:else if param.type === 5 && param.options}
              <!-- Enum parameter -->
              <select
                class="w-full p-2 border rounded"
                value={param.value}
                onchange={(e) => handleSelectChange(e, param.name)}
              >
                {#each param.options as option}
                  <option value={option.value}>{option.label}</option>
                {/each}
              </select>
            {:else if param.type === 4}
              <!-- String parameter -->
              <input
                type="text"
                class="w-full p-2 border rounded"
                value={param.value}
                oninput={(e) => handleTextInput(e, param.name)}
              />
            {:else}
              <!-- Numeric parameter -->
              <input
                type="number"
                class="w-full p-2 border rounded"
                value={param.value}
                min={param.min}
                max={param.max}
                step={param.type === 2 || param.type === 3 ? 0.1 : 1}
                oninput={(e) => handleNumberInput(e, param.name)}
              />
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
