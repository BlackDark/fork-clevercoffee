<script lang="ts">
  import { onMount } from 'svelte';
  import { getParameterLabel } from '../../parameter-labels';
  import { apiJsonFetch } from '../../api-config';
  import { routeParams } from '../../router';
  import { basePath } from '../../config';

  // Parameter data
  let parameters: any[] = [];
  let loading = true;
  let error: string | null = null;

  // Filter categories
  const categories = [
    { id: 'all', label: 'All Parameters' },
    { id: 'brew', label: 'Brew' },
    { id: 'pid', label: 'PID' },
    { id: 'steam', label: 'Steam' },
    { id: 'hardware', label: 'Hardware' },
    { id: 'system', label: 'System' }
  ];

  $: activeFilter = $routeParams.filter || 'all';

  $: filteredParameters = parameters.filter(param => {
    if (activeFilter === 'all') return true;
    return param.name.startsWith(activeFilter);
  });

  async function fetchParameters() {
    try {
      const data = await apiJsonFetch<any[]>('parameters');
      parameters = data;
      error = null;
    } catch (err) {
      console.error('Failed to fetch parameters:', err);
      error = 'Failed to fetch configuration parameters';
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchParameters();
  });
</script>

<div>
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

  {#if loading}
    <div class="animate-pulse space-y-4">
      {#each Array(5) as _}
        <div class="h-16 bg-muted rounded"></div>
      {/each}
    </div>
  {:else if error}
    <div class="p-4 bg-red-100 text-red-700 rounded-md">{error}</div>
  {:else if filteredParameters.length === 0}
    <div class="p-4 bg-muted rounded-md">No parameters found for this filter.</div>
  {:else}
    <div class="space-y-4">
      {#each filteredParameters as param}
        <div class="p-4 bg-card rounded-md border">
          <div class="font-medium">{getParameterLabel(param.name)}</div>
          <div class="text-sm text-muted-foreground">{param.name}</div>
          <div class="mt-2">
            <!-- Parameter value display/edit would go here -->
            <div class="text-sm">Value: {param.value}</div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
