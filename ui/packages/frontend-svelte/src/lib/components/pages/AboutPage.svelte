<script lang="ts">
  import { onMount } from 'svelte';
  import { apiJsonFetch } from '../../api-config';

  let version = 'Unknown';
  let loading = true;

  onMount(async () => {
    try {
      const data = await apiJsonFetch<{version?: string}>('version');
      version = data.version || 'Unknown';
    } catch (err) {
      console.error('Failed to fetch version:', err);
    } finally {
      loading = false;
    }
  });
</script>

<div>
  <h1 class="text-3xl font-bold mb-6">About</h1>

  <div class="space-y-6">
    <div class="p-6 bg-card rounded-lg border">
      <h2 class="text-xl font-semibold mb-4">CleverCoffee</h2>

      {#if loading}
        <div class="animate-pulse h-4 w-24 bg-muted rounded"></div>
      {:else}
        <p class="mb-4">Version: {version}</p>
      {/if}

      <p class="mb-4">
        CleverCoffee is an open-source project for espresso machine control and monitoring.
      </p>

      <p>
        <a
          href="https://github.com/clevercoffee/clevercoffee"
          target="_blank"
          rel="noopener noreferrer"
          class="text-primary hover:underline"
        >
          GitHub Repository
        </a>
      </p>
    </div>

    <div class="p-6 bg-card rounded-lg border">
      <h2 class="text-xl font-semibold mb-4">Contributors</h2>
      <p>
        This project is made possible by the contributions of many individuals.
        Visit our GitHub repository to see the full list of contributors.
      </p>
    </div>

    <div class="p-6 bg-card rounded-lg border">
      <h2 class="text-xl font-semibold mb-4">License</h2>
      <p>
        This project is licensed under the MIT License.
      </p>
    </div>
  </div>
</div>
