<script lang="ts">
  import { onMount } from 'svelte';
  import { apiJsonFetch } from '../../api-config';
  import { toast } from 'svelte-sonner';

  let systemInfo: any = null;
  let loading = true;
  let error: string | null = null;

  async function fetchSystemInfo() {
    try {
      const data = await apiJsonFetch('system/info');
      systemInfo = data;
      error = null;
    } catch (err) {
      console.error('Failed to fetch system info:', err);
      error = 'Failed to fetch system information';
    } finally {
      loading = false;
    }
  }

  async function restartSystem() {
    if (!confirm('Are you sure you want to restart the system?')) {
      return;
    }

    try {
      await apiJsonFetch('system/restart', { method: 'POST' });
      toast.success('System restart initiated');
    } catch (err) {
      console.error('Failed to restart system:', err);
      toast.error('Failed to restart system');
    }
  }

  onMount(() => {
    fetchSystemInfo();
  });
</script>

<div>
  <h1 class="text-3xl font-bold mb-6">System</h1>

  {#if loading}
    <div class="animate-pulse space-y-4">
      <div class="h-8 w-1/3 bg-muted rounded"></div>
      <div class="h-32 bg-muted rounded"></div>
    </div>
  {:else if error}
    <div class="p-4 bg-red-100 text-red-700 rounded-md">{error}</div>
  {:else if systemInfo}
    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
      <div class="p-6 bg-card rounded-lg border">
        <h2 class="text-xl font-semibold mb-4">System Information</h2>
        <dl class="space-y-2">
          <div class="flex justify-between">
            <dt class="font-medium">Version</dt>
            <dd>{systemInfo.version || 'Unknown'}</dd>
          </div>
          <div class="flex justify-between">
            <dt class="font-medium">Uptime</dt>
            <dd>{systemInfo.uptime || 'Unknown'}</dd>
          </div>
          <div class="flex justify-between">
            <dt class="font-medium">Free Memory</dt>
            <dd>{systemInfo.freeMemory || 'Unknown'} KB</dd>
          </div>
        </dl>
      </div>

      <div class="p-6 bg-card rounded-lg border">
        <h2 class="text-xl font-semibold mb-4">System Actions</h2>
        <div class="space-y-4">
          <button
            class="w-full px-4 py-2 bg-red-500 text-white rounded-md hover:bg-red-600"
            on:click={restartSystem}
          >
            Restart System
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
