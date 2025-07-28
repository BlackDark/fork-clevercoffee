<script lang="ts">
	import { cleverCoffeeState } from '$lib/stores/clever-coffee-store.svelte';
	import { Wifi, WifiOff } from 'lucide-svelte';
</script>

<div class="flex items-center gap-2 text-sm">
	<div class="flex items-center gap-1">
		{#if cleverCoffeeState.isOnline}
			<Wifi class="h-4 w-4 text-green-600" />
			<span class="font-medium text-green-600">Online</span>
			<div class="h-2 w-2 animate-pulse rounded-full bg-green-500"></div>
		{:else}
			<WifiOff class="h-4 w-4 text-red-600" />
			<span class="font-medium text-red-600">Offline</span>
			<div class="h-2 w-2 rounded-full bg-red-500"></div>
		{/if}
	</div>
</div>
