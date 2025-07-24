<script lang="ts">
	import { Moon, Sun } from 'lucide-svelte';
	import Button from '$lib/components/ui/button/button.svelte';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger
	} from '$lib/components/ui/dropdown-menu';
	import { getTheme, setTheme } from '$lib/stores/theme-store.svelte';

	// Make currentTheme reactive using $derived
	const currentTheme = $derived(getTheme());
</script>

<DropdownMenu>
	<DropdownMenuTrigger>
		<Button variant="outline" size="icon">
			<Sun
				class="h-[1.2rem] w-[1.2rem] rotate-0 scale-100 transition-all dark:-rotate-90 dark:scale-0"
			/>
			<Moon
				class="absolute h-[1.2rem] w-[1.2rem] rotate-90 scale-0 transition-all dark:rotate-0 dark:scale-100"
			/>
			<span class="sr-only">Toggle theme</span>
		</Button>
	</DropdownMenuTrigger>
	<DropdownMenuContent align="end">
		<DropdownMenuItem onclick={() => setTheme('light')}>
			Light {currentTheme === 'light' ? '✓' : ''}
		</DropdownMenuItem>
		<DropdownMenuItem onclick={() => setTheme('dark')}>
			Dark {currentTheme === 'dark' ? '✓' : ''}
		</DropdownMenuItem>
		<DropdownMenuItem onclick={() => setTheme('system')}>
			System {currentTheme === 'system' ? '✓' : ''}
		</DropdownMenuItem>
	</DropdownMenuContent>
</DropdownMenu>
