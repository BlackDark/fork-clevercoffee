<script lang="ts">
  import { onMount } from 'svelte';
  import { apiJsonFetch } from '../../api-config';

  let temperature = 0;
  let loading = true;
  let error: string | null = null;

  async function fetchTemperature() {
    try {
      const data = await apiJsonFetch<{ temperature: number }>('temperature');
      temperature = data.temperature;
      error = null;
    } catch (err) {
      console.error('Failed to fetch temperature:', err);
      error = 'Failed to fetch temperature data';
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    fetchTemperature();

    // Set up polling for temperature updates
    const interval = setInterval(fetchTemperature, 5000);

    return () => {
      clearInterval(interval);
    };
  });
</script>

<div>
  <h1 class="text-3xl font-bold mb-6">Coffee Machine Dashboard</h1>

  <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
    <div class="p-6 bg-card rounded-lg shadow">
      <h2 class="text-xl font-semibold mb-4">Current Temperature</h2>

      {#if loading}
        <div class="animate-pulse flex space-x-4">
          <div class="h-12 w-full bg-muted rounded"></div>
        </div>
      {:else if error}
        <div class="text-red-500">{error}</div>
      {:else}
        <div class="text-4xl font-bold">{temperature.toFixed(1)}°C</div>
      {/if}
    </div>

    <!-- Additional dashboard cards can be added here -->
  </div>
</div>
