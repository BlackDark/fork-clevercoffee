<script lang="ts">
  import { Card, CardContent, CardHeader, CardTitle } from '$lib/components/ui/card';
  import { Globe, Github, MessageCircle, Info } from 'lucide-svelte';
  import { onMount } from 'svelte';
  import { apiJsonFetch } from '$lib/api-config';

  let cleverCoffeeVersion = 'Unknown';
  let loading = true;

  onMount(async () => {
    try {
      const data = await apiJsonFetch<{version?: string}>('version');
      cleverCoffeeVersion = data.version || 'Unknown';
    } catch (err) {
      console.error('Failed to fetch CleverCoffee version:', err);
    } finally {
      loading = false;
    }
  });

  const uiVersion = '2.0.0-beta'; // Hardcoded as in the React version
</script>

<div class="container mx-auto p-6 space-y-6 max-w-7xl">
  <!-- Version Information -->
  <Card>
    <CardHeader>
      <CardTitle class="flex items-center gap-3">
        <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-blue-500/10">
          <Info class="h-5 w-5 text-blue-600" />
        </div>
        Version Information
      </CardTitle>
    </CardHeader>
    <CardContent>
      <div class="space-y-2">
        <div class="flex items-center justify-between">
          <span class="text-muted-foreground">
            CleverCoffee Version
          </span>
          {#if loading}
            <div class="animate-pulse h-4 w-24 bg-muted px-2 py-1 rounded"></div>
          {:else}
            <span class="font-mono text-sm bg-muted px-2 py-1 rounded">
              {cleverCoffeeVersion}
            </span>
          {/if}
        </div>
        <div class="flex items-center justify-between">
          <span class="text-muted-foreground">UI Version</span>
          <span class="font-mono text-sm bg-muted px-2 py-1 rounded">
            {uiVersion}
          </span>
        </div>
      </div>
    </CardContent>
  </Card>

  <!-- Community & Resources -->
  <Card>
    <CardHeader>
      <CardTitle class="flex items-center gap-3">
        <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-green-500/10">
          <MessageCircle class="h-5 w-5 text-green-600" />
        </div>
        Community & Resources
      </CardTitle>
    </CardHeader>
    <CardContent>
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <a
          href="https://clevercoffee.de/"
          class="block"
          target="_blank"
          rel="noopener noreferrer"
        >
          <div class="flex flex-col items-center justify-center p-6 rounded-lg border bg-card hover:bg-accent/50 transition-colors duration-200 h-32">
            <Globe class="h-8 w-8 mb-3 text-blue-600" />
            <p class="font-medium text-center">Project Website</p>
            <p class="text-xs text-muted-foreground text-center mt-1">
              clevercoffee.de
            </p>
          </div>
        </a>
        <a
          href="https://github.com/rancilio-pid/clevercoffee"
          class="block"
          target="_blank"
          rel="noopener noreferrer"
        >
          <div class="flex flex-col items-center justify-center p-6 rounded-lg border bg-card hover:bg-accent/50 transition-colors duration-200 h-32">
            <Github class="h-8 w-8 mb-3 text-gray-700 dark:text-gray-300" />
            <p class="font-medium text-center">GitHub Repository</p>
            <p class="text-xs text-muted-foreground text-center mt-1">
              Source code & issues
            </p>
          </div>
        </a>
        <a
          href="https://discord.gg/Kq5RFznuU4"
          class="block"
          target="_blank"
          rel="noopener noreferrer"
        >
          <div class="flex flex-col items-center justify-center p-6 rounded-lg border bg-card hover:bg-accent/50 transition-colors duration-200 h-32">
            <MessageCircle class="h-8 w-8 mb-3 text-purple-600" />
            <p class="font-medium text-center">Discord Community</p>
            <p class="text-xs text-muted-foreground text-center mt-1">
              Get help & support
            </p>
          </div>
        </a>
      </div>
    </CardContent>
  </Card>

  <!-- Project Information -->
  <Card>
    <CardHeader>
      <CardTitle class="flex items-center gap-3">
        <div class="flex h-10 w-10 items-center justify-center rounded-lg bg-orange-500/10">
          <Info class="h-5 w-5 text-orange-600" />
        </div>
        About CleverCoffee
      </CardTitle>
    </CardHeader>
    <CardContent class="space-y-4">
      <p class="text-muted-foreground">
        CleverCoffee is an open-source PID controller firmware for espresso
        machines, providing precise temperature control and advanced brewing
        features. This modern web interface offers real-time monitoring,
        parameter configuration, and system management.
      </p>
      <div class="grid grid-cols-1 md:grid-cols-2 gap-4 pt-4">
        <div class="space-y-2">
          <h4 class="font-medium">Key Features</h4>
          <ul class="text-sm text-muted-foreground space-y-1">
            <li>• PID temperature control</li>
            <li>• Real-time monitoring</li>
            <li>• Parameter management</li>
            <li>• System configuration</li>
          </ul>
        </div>
        <div class="space-y-2">
          <h4 class="font-medium">Support</h4>
          <ul class="text-sm text-muted-foreground space-y-1">
            <li>• Community Discord server</li>
            <li>• GitHub issues & discussions</li>
            <li>• Project documentation</li>
            <li>• Open source contributions</li>
          </ul>
        </div>
      </div>
    </CardContent>
  </Card>
</div>
