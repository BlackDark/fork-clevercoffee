<script lang="ts">
  import { page } from '$app/stores';
  import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
  import { Button } from '$lib/components/ui/button';
  import { AlertCircle, Home, ArrowLeft } from 'lucide-svelte';
  import { onMount, onDestroy } from 'svelte';
  import { goto } from '$app/navigation';
  import { basePath } from '$lib/config';

  let status = $page.status;
  let error = $page.error;

  onMount(() => {
    // Set document title for 404 page
    document.title = "404 - Page Not Found | CleverCoffee";

    // Add meta tag for robots (prevent indexing of 404 pages)
    const metaRobots = document.createElement("meta");
    metaRobots.name = "robots";
    metaRobots.content = "noindex, nofollow";
    document.head.appendChild(metaRobots);

    // Log 404 for debugging/analytics
    console.warn(`404 - Page not found: ${window.location.pathname}`);

    // Cleanup on unmount
    return () => {
      document.title = "CleverCoffee";
      document.head.removeChild(metaRobots);
    };
  });

  function goBack() {
    window.history.back();
  }

  function goToHome() {
    goto(basePath);
  }
</script>

<div class="container mx-auto p-6 space-y-6 max-w-7xl">
  <div class="flex items-center justify-center min-h-[60vh]">
    <Card class="w-full max-w-lg">
      <CardHeader class="text-center">
        <div class="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-lg bg-red-500/10">
          <AlertCircle class="h-8 w-8 text-red-600" />
        </div>
        <CardTitle class="text-3xl font-bold">{status}</CardTitle>
        <p class="text-lg text-muted-foreground">Page Not Found</p>
      </CardHeader>
      <CardContent class="text-center space-y-6">
        <div class="space-y-2">
          <h2 class="text-xl font-semibold">
            Oops! This page doesn't exist
          </h2>
          <p class="text-muted-foreground">
            The page you're looking for might have been moved, deleted, or
            you may have entered an incorrect URL.
          </p>
        </div>

        <div class="flex flex-col sm:flex-row gap-3 justify-center">
          <Button on:click={goToHome}>
            <Home class="mr-2 h-4 w-4" />
            Go to Home
          </Button>
          <Button variant="outline" on:click={goBack}>
            <ArrowLeft class="mr-2 h-4 w-4" />
            Go Back
          </Button>
        </div>

        <div class="pt-4 border-t">
          <p class="text-sm text-muted-foreground mb-3">
            Available pages:
          </p>
          <div class="flex flex-wrap gap-2 justify-center">
            <a href="{basePath}/" class="text-sm text-primary hover:underline">
              Home
            </a>
            <span class="text-muted-foreground">•</span>
            <a
              href="{basePath}/config/behavior"
              class="text-sm text-primary hover:underline"
            >
              Parameters
            </a>
            <span class="text-muted-foreground">•</span>
            <a
              href="{basePath}/system"
              class="text-sm text-primary hover:underline"
            >
              System
            </a>
            <span class="text-muted-foreground">•</span>
            <a
              href="{basePath}/about"
              class="text-sm text-primary hover:underline"
            >
              About
            </a>
          </div>
        </div>
      </CardContent>
    </Card>
  </div>
</div>
