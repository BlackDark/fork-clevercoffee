<script lang="ts">
	import '../app.css';
	import { page } from '$app/state';
	import { cn } from '$lib/utils';
	import { Menu, X } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';
	import { basePathWithoutTrailingSlash } from '$lib/config';
	import LiveStatusIndicator from '$lib/components/LiveStatusIndicator.svelte';
	import ModeToggle from '$lib/components/mode-toggle.svelte';

	import { getTheme, updateThemeClass, type Theme } from '$lib/stores/theme-store.svelte';
	import { browser } from '$app/environment';
	import { connectEventSource } from '$lib/stores/clever-coffee-store.svelte';

	const currentTheme = $derived(getTheme());

	$effect(() => {
		if (browser) {
			localStorage.setItem('theme', currentTheme);
			updateThemeClass(currentTheme);

			// Listen for system theme changes
			window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
				const currentTheme = localStorage.getItem('theme') as Theme;
				if (currentTheme === 'system') {
					updateThemeClass('system');
				}
			});
		}
	});

	connectEventSource();

	let { children } = $props();

	let isMobileMenuOpen = $state(false);

	// Derived reactive value for current pathname
	const pathname = $derived(page.url.pathname);

	function isActive(path: string) {
		if (path === '/') {
			return pathname === '/';
		}
		return pathname.startsWith(path);
	}

	const navigationItems = [
		{ path: '/', label: 'Home' },
		{ path: '/config/behavior', label: 'Configuration' },
		{ path: '/system', label: 'System' },
		{ path: '/about', label: 'About' }
	];
	console.log('Navigation items:', navigationItems);

	function closeMobileMenu() {
		isMobileMenuOpen = false;
	}

	function toggleMobileMenu() {
		isMobileMenuOpen = !isMobileMenuOpen;
	}

	// Close mobile menu when pathname changes
	$effect(() => {
		pathname;
		isMobileMenuOpen = false;
	});
</script>

<div class="flex min-h-screen w-full flex-col">
	<header
		class="bg-background sticky top-0 z-50 flex h-16 items-center justify-between gap-4 border-b px-4 md:px-6"
	>
		<!-- Desktop Navigation -->
		<nav
			class="hidden flex-col gap-6 text-lg font-medium md:flex md:flex-row md:items-center md:gap-5 md:text-sm lg:gap-6"
		>
			<a href="/" class="flex items-center gap-2 text-lg font-semibold md:text-base">
				<img src={`${basePathWithoutTrailingSlash}/logo.png`} class="h-6 w-6" alt="Logo" />
				<span class="sr-only">CleverCoffee</span>
			</a>
			{#each navigationItems as item (item.path)}
				<a
					href={item.path === '/settings/behavior' ? '/settings/behavior' : item.path}
					class={cn(
						'hover:text-foreground font-medium transition-colors',
						isActive(item.path === '/settings/behavior' ? '/settings' : item.path)
							? 'text-foreground'
							: 'text-muted-foreground'
					)}
				>
					{item.label}
				</a>
			{/each}
		</nav>

		<!-- Mobile Navigation -->
		<div class="flex items-center gap-4 md:hidden">
			<a href="/" class="flex items-center gap-2 text-lg font-semibold">
				<img src="/vite.svg" class="h-6 w-6" alt="Logo" />
				<span class="sr-only">CleverCoffee</span>
			</a>
		</div>

		<div class="flex items-center gap-2">
			<LiveStatusIndicator />
			<ModeToggle />

			<!-- Mobile Menu Button -->
			<Button
				variant="ghost"
				size="icon"
				class="md:hidden"
				onclick={toggleMobileMenu}
				aria-label="Toggle menu"
			>
				{#if isMobileMenuOpen}
					<X class="h-5 w-5" />
				{:else}
					<Menu class="h-5 w-5" />
				{/if}
			</Button>
		</div>
	</header>

	<!-- Mobile Menu Overlay -->
	{#if isMobileMenuOpen}
		<div class="bg-background/80 fixed inset-0 top-16 z-40 backdrop-blur-sm md:hidden">
			<nav class="bg-background flex flex-col gap-2 border-b p-4 shadow-lg">
				{#each navigationItems as item (item.path)}
					<a
						href={item.path === '/settings/behavior' ? '/settings/behavior' : item.path}
						class={cn(
							'hover:bg-accent hover:text-accent-foreground flex items-center gap-2 rounded-lg px-3 py-2 text-sm font-medium transition-colors',
							isActive(item.path === '/settings/behavior' ? '/settings' : item.path)
								? 'bg-accent text-accent-foreground'
								: 'text-muted-foreground'
						)}
						onclick={closeMobileMenu}
					>
						{item.label}
					</a>
				{/each}
			</nav>
		</div>
	{/if}

	<main class="flex flex-1 flex-col overflow-auto">
		{@render children()}
	</main>
</div>
