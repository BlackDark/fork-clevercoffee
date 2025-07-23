<script lang="ts">
  import LiveStatusIndicator from './LiveStatusIndicator.svelte';
  import ThemeToggle from './ThemeToggle.svelte';
  import { Menu, X } from '@lucide/svelte';
  import { basePath, basePathWithoutTrailingSlash } from '../config';
  import { cn } from '../utils';
  import { onMount } from 'svelte';
  import { connectEventSource } from '../stores/clever-coffee-store';

  // Navigation items
  const navigationItems = [
    { path: '/', label: 'Home' },
    { path: '/config/behavior', label: 'Configuration' },
    { path: '/system', label: 'System' },
    { path: '/about', label: 'About' },
  ];

  // Mobile menu state
  let isMobileMenuOpen = false;

  // Get current path for active link highlighting
  let currentPath = '';

  // Update current path when component mounts
  onMount(() => {
    currentPath = window.location.pathname;

    // Listen for navigation events to update active state
    const handleNavigation = () => {
      currentPath = window.location.pathname;
    };

    window.addEventListener('popstate', handleNavigation);

    return () => {
      window.removeEventListener('popstate', handleNavigation);
    };
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

  // Connect to event source on mount
  onMount(() => {
    const { disconnect } = connectEventSource();
    return disconnect;
  });
</script>

<div class="flex min-h-screen w-full flex-col">
  <header class="sticky top-0 flex h-16 items-center gap-4 border-b bg-background px-4 md:px-6 justify-between z-50">
    <!-- Desktop Navigation -->
    <nav class="hidden flex-col gap-6 text-lg font-medium md:flex md:flex-row md:items-center md:gap-5 md:text-sm lg:gap-6">
      <a
        href="{basePath}"
        class="flex items-center gap-2 text-lg font-semibold md:text-base"
      >
        <img
          src="{basePathWithoutTrailingSlash}/logo.png"
          class="h-6 w-6"
          alt="Logo"
        />
        <span class="sr-only">CleverCoffee</span>
      </a>
      {#each navigationItems as item}
        <a
          href="{basePath + (item.path === '/' ? '' : item.path)}"
          class={cn(
            "transition-colors hover:text-foreground font-medium",
            isActive(item.path === "/settings/behavior" ? "/settings" : item.path)
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
        href="{basePath}"
        class="flex items-center gap-2 text-lg font-semibold"
      >
        <img src="/vite.svg" class="h-6 w-6" alt="Logo" />
        <span class="sr-only">CleverCoffee</span>
      </a>
    </div>

    <div class="flex items-center gap-2">
      <LiveStatusIndicator />
      <ThemeToggle />

      <!-- Mobile Menu Button -->
      <button
        class="inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:opacity-50 disabled:pointer-events-none ring-offset-background hover:bg-accent hover:text-accent-foreground h-10 w-10 md:hidden"
        on:click={() => isMobileMenuOpen = !isMobileMenuOpen}
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
            href="{basePath + (item.path === '/' ? '' : item.path)}"
            class={cn(
              "flex items-center gap-2 rounded-lg px-3 py-2 text-sm font-medium transition-colors hover:bg-accent hover:text-accent-foreground",
              isActive(item.path === "/settings/behavior" ? "/settings" : item.path)
                ? "bg-accent text-accent-foreground"
                : "text-muted-foreground"
            )}
            on:click={closeMobileMenu}
          >
            {item.label}
          </a>
        {/each}
      </nav>
    </div>
  {/if}

  <main class="flex flex-1 flex-col overflow-auto">
    <slot />
  </main>
</div>
