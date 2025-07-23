<script lang="ts">
  import { Wifi, WifiOff } from 'lucide-svelte';
  import { cleverCoffeeState } from '../stores/clever-coffee-store.svelte';
</script>

<div class="flex items-center gap-2 text-sm">
  <div class="flex items-center gap-1">
    {#if cleverCoffeeState.isOnline}
      <Wifi class="h-4 w-4 text-green-600" />
      <span class="text-green-600 font-medium">Online</span>
      <div class="h-2 w-2 bg-green-500 rounded-full animate-pulse"></div>
    {:else}
      <WifiOff class="h-4 w-4 text-red-600" />
      <span class="text-red-600 font-medium">Offline</span>
      <div class="h-2 w-2 bg-red-500 rounded-full"></div>
    {/if}
  </div>
</div>
