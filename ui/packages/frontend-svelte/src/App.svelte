<script lang="ts">
  import './app.css';
  import { Toaster } from 'svelte-sonner';
  import { onMount } from 'svelte';
  import { currentComponent, initRouter } from './lib/router';

  // Layout and Pages
  import Layout from './lib/components/Layout.svelte';
  import HomePage from './lib/components/pages/HomePage.svelte';
  import ConfigPage from './lib/components/pages/ConfigPage.svelte';
  import SystemPage from './lib/components/pages/SystemPage.svelte';
  import AboutPage from './lib/components/pages/AboutPage.svelte';
  import NotFoundPage from './lib/components/pages/NotFoundPage.svelte';

  // Initialize router on mount
  onMount(() => {
    initRouter();
  });
</script>

<Layout>
  {#if $currentComponent === 'home'}
    <HomePage />
  {:else if $currentComponent === 'config'}
    <ConfigPage />
  {:else if $currentComponent === 'system'}
    <SystemPage />
  {:else if $currentComponent === 'about'}
    <AboutPage />
  {:else}
    <NotFoundPage />
  {/if}
</Layout>

<Toaster />
