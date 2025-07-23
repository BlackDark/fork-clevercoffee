<script lang="ts">
	import '../app.css';
	import LiveStatusIndicator from '$lib/components/LiveStatusIndicator.svelte';
	import ThemeToggle from '$lib/components/ThemeToggle.svelte';
	import { Menu, X } from 'lucide-svelte';
	import { basePath, basePathWithoutTrailingSlash } from '$lib/config';
	import { cn } from '$lib/utils';
	import { onMount, onDestroy } from 'svelte';
	import { connectEventSource } from '$lib/stores/clever-coffee-store.svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';

	let { children } = $props();

	// Navigation items
	const navigationItems = [
		{ path: './', label: 'Home' },
		{ path: './config/behavior', label: 'Configuration' },
		{ path: './system', label: 'System' },
		{ path: './about', label: 'About' },
	];

	// Mobile menu state using reactive variable
	let isMobileMenuOpen = $state(false);

	// Track current path for active link highlighting
	let currentPath = $state('');

	// Update current path when page changes
	$effect(() => {
		currentPath = $page.url.pathname;
		console.log('Current path:', currentPath);
	});

	// Determine if a nav item is active
	function isActive(path: string): boolean {
		// Normalize current path by removing basePath
		const normalizedPath = currentPath.startsWith(basePath)
			? currentPath.substring(basePath.length) || '/'
			: currentPath;

		if (path === '/') {
			return normalizedPath === '/' || normalizedPath === '';
		}

		return normalizedPath.startsWith(path);
	}

	// Close mobile menu
	function closeMobileMenu() {
		isMobileMenuOpen = false;
	}

	// Toggle mobile menu
	function toggleMobileMenu() {
		isMobileMenuOpen = !isMobileMenuOpen;
	}

	// Handle navigation with proper base path
	function handleNavigation(event: Event, path: string) {
		event.preventDefault();
		const fullPath = basePath + (path === '/' ? '' : path);
		console.log('Navigating to:', fullPath);
		goto(fullPath, { replaceState: false });
		closeMobileMenu();
	}

	// Event source connection
	let eventSourceConnection: { disconnect: () => void } | null = null;

	// Connect to event source on mount
	onMount(() => {
		console.log('Layout mounted, base path:', basePath);
		eventSourceConnection = connectEventSource();

		// Add event listener for navigation
		window.addEventListener('popstate', () => {
			console.log('Popstate event, current location:', window.location.pathname);
			currentPath = window.location.pathname;
		});

		return () => {
			window.removeEventListener('popstate', () => {});
		};
	});

	// Disconnect event source on destroy
	onDestroy(() => {
		if (eventSourceConnection) {
			eventSourceConnection.disconnect();
		}
	});
</script>

<div class="flex min-h-screen w-full flex-col">
	<header class="sticky top-0 flex h-16 items-center gap-4 border-b bg-background px-4 md:px-6 justify-between z-50">
		<!-- Desktop Navigation -->
		<nav class="hidden flex-col gap-6 text-lg font-medium md:flex md:flex-row md:items-center md:gap-5 md:text-sm lg:gap-6">
			<a
				href="/"
				data-sveltekit-preload-data="hover"
				onclick={(e) => handleNavigation(e, '/')}
				class="flex items-center gap-2 text-lg font-semibold md:text-base"
			>
				<img
					src="/logo.png"
					class="h-6 w-6"
					alt="Logo"
				/>
				<span>CleverCoffee</span>
			</a>
			{#each navigationItems as item}
				<a
					href={item.path}
					data-sveltekit-preload-data="hover"
					onclick={(e) => handleNavigation(e, item.path)}
					class={cn(
						"transition-colors hover:text-foreground font-medium",
						isActive(item.path)
							? "text-foreground"
							: "text-muted-foreground"
					)}
				>
					{item.label}
				</a>
			{/each}
		</nav>

		<!-- Mobile Navigation -->
		<div class="flex items-center gap-4 md:hidden">
			<a
				href="/"
				data-sveltekit-preload-data="hover"
				onclick={(e) => handleNavigation(e, '/')}
				class="flex items-center gap-2 text-lg font-semibold"
			>
				<img src="/logo.png" class="h-6 w-6" alt="Logo" />
				<span>CleverCoffee</span>
			</a>
		</div>

		<div class="flex items-center gap-2">
			<LiveStatusIndicator />
			<ThemeToggle />

			<!-- Mobile Menu Button -->
			<button
				class="inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:opacity-50 disabled:pointer-events-none ring-offset-background hover:bg-accent hover:text-accent-foreground h-10 w-10 md:hidden"
				onclick={toggleMobileMenu}
				aria-label="Toggle menu"
			>
				{#if isMobileMenuOpen}
					<X class="h-5 w-5" />
				{:else}
					<Menu class="h-5 w-5" />
				{/if}
			</button>
		</div>
	</header>

	<!-- Mobile Menu Overlay -->
	{#if isMobileMenuOpen}
		<div class="fixed inset-0 top-16 z-40 bg-background/80 backdrop-blur-sm md:hidden">
			<nav class="flex flex-col gap-2 p-4 bg-background border-b shadow-lg">
				{#each navigationItems as item}
					<a
						href={item.path}
						data-sveltekit-preload-data="hover"
						onclick={(e) => handleNavigation(e, item.path)}
						class={cn(
							"flex items-center gap-2 rounded-lg px-3 py-2 text-sm font-medium transition-colors hover:bg-accent hover:text-accent-foreground",
							isActive(item.path)
								? "bg-accent text-accent-foreground"
								: "text-muted-foreground"
						)}
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
