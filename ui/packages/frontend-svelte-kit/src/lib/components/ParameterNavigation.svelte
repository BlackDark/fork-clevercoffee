<script lang="ts">
	import { page } from '$app/stores';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger
	} from '$lib/components/ui/dropdown-menu';
	import { Button } from '$lib/components/ui/button';
	import { ChevronDown, Settings } from 'lucide-svelte';

	const parameterCategories = [
		{
			filter: 'behavior',
			label: 'Behavior & Control',
			description: 'PID, brewing, temperature, and control settings'
		},
		{
			filter: 'hardware',
			label: 'Hardware Configuration',
			description: 'Sensors, relays, display, and hardware settings'
		},
		{
			filter: 'system',
			label: 'System & Connectivity',
			description: 'MQTT, power management, and system configuration'
		}
	];

	let currentCategory: { filter: string; label: string; description: string } | undefined;
	page.subscribe((p) => {
		currentCategory = parameterCategories.find((cat) => cat.filter === p.params.filter);
	});
</script>

<div class="mb-6 flex items-center gap-2">
	<Settings class="text-muted-foreground h-5 w-5" />
	<span class="text-muted-foreground text-sm font-medium">Parameter Category:</span>

	<DropdownMenu>
		<DropdownMenuTrigger>
			<Button variant="outline" class="min-w-[200px] justify-between">
				{currentCategory?.label || 'Select Category'}
				<ChevronDown class="ml-2 h-4 w-4" />
			</Button>
		</DropdownMenuTrigger>
		<DropdownMenuContent align="start" class="w-80">
			{#each parameterCategories as category}
				<DropdownMenuItem>
					<a
						href={`/config/${category.filter}`}
						class="flex cursor-pointer flex-col items-start p-3"
					>
						<div class="font-medium">{category.label}</div>
						<div class="text-muted-foreground text-xs">{category.description}</div>
					</a>
				</DropdownMenuItem>
			{/each}
		</DropdownMenuContent>
	</DropdownMenu>
</div>
