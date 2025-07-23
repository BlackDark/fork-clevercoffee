<script lang="ts">
  import { theme, setTheme } from '../stores/theme-store';
  import { Moon, Sun, Laptop } from 'lucide-svelte';

  function setLightTheme() {
    setTheme('light');
  }

  function setDarkTheme() {
    setTheme('dark');
  }

  function setSystemTheme() {
    setTheme('system');
  }
</script>

<div class="flex items-center space-x-2">
  <button
    class="inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:opacity-50 disabled:pointer-events-none ring-offset-background hover:bg-accent hover:text-accent-foreground h-10 w-10"
    onclick={setLightTheme}
    aria-label="Light mode"
    class:text-primary={$theme === 'light'}
    class:text-muted-foreground={$theme !== 'light'}
  >
    <Sun class="h-5 w-5" />
    <span class="sr-only">Light mode</span>
  </button>

  <button
    class="inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:opacity-50 disabled:pointer-events-none ring-offset-background hover:bg-accent hover:text-accent-foreground h-10 w-10"
    onclick={setDarkTheme}
    aria-label="Dark mode"
    class:text-primary={$theme === 'dark'}
    class:text-muted-foreground={$theme !== 'dark'}
  >
    <Moon class="h-5 w-5" />
    <span class="sr-only">Dark mode</span>
  </button>

  <button
    class="inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:opacity-50 disabled:pointer-events-none ring-offset-background hover:bg-accent hover:text-accent-foreground h-10 w-10"
    onclick={setSystemTheme}
    aria-label="System mode"
    class:text-primary={$theme === 'system'}
    class:text-muted-foreground={$theme !== 'system'}
  >
    <Laptop class="h-5 w-5" />
    <span class="sr-only">System mode</span>
  </button>
</div>
