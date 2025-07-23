<script lang="ts">
  import { theme, setTheme } from '../stores/theme-store';
  import { Moon, Sun, Laptop } from '@lucide/svelte';
</script>

<div class="flex items-center space-x-2">
  <button
    class="inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:opacity-50 disabled:pointer-events-none ring-offset-background hover:bg-accent hover:text-accent-foreground h-10 w-10"
    on:click={() => setTheme('light')}
    aria-label="Light mode"
    class:text-primary={$theme === 'light'}
    class:text-muted-foreground={$theme !== 'light'}
  >
    <Sun class="h-5 w-5" />
    <span class="sr-only">Light mode</span>
  </button>

  <button
    class="inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:opacity-50 disabled:pointer-events-none ring-offset-background hover:bg-accent hover:text-accent-foreground h-10 w-10"
    on:click={() => setTheme('dark')}
    aria-label="Dark mode"
    class:text-primary={$theme === 'dark'}
    class:text-muted-foreground={$theme !== 'dark'}
  >
    <Moon class="h-5 w-5" />
    <span class="sr-only">Dark mode</span>
  </button>

  <button
    class="inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:opacity-50 disabled:pointer-events-none ring-offset-background hover:bg-accent hover:text-accent-foreground h-10 w-10"
    on:click={() => setTheme('system')}
    aria-label="System mode"
    class:text-primary={$theme === 'system'}
    class:text-muted-foreground={$theme !== 'system'}
  >
    <Laptop class="h-5 w-5" />
    <span class="sr-only">System mode</span>
  </button>
</div>
