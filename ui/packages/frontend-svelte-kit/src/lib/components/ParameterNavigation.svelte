<script lang="ts">
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
  } from './ui/dropdown-menu';
  import { Button } from './ui/button';
  import { ChevronDown, Settings } from 'lucide-svelte';
  import { basePath } from '$lib/config';

  const parameterCategories = [
    {
      filter: "behavior",
      label: "Behavior & Control",
      description: "PID, brewing, temperature, and control settings",
    },
    {
      filter: "hardware",
      label: "Hardware Configuration",
      description: "Sensors, relays, display, and hardware settings",
    },
    {
      filter: "system",
      label: "System & Connectivity",
      description: "MQTT, power management, and system configuration",
    },
  ];

  let currentFilter = $state($page.params.filter);

  $effect(() => {
    currentFilter = $page.params.filter;
  });

  const currentCategory = $derived(
    parameterCategories.find((cat) => cat.filter === currentFilter)
  );

  function navigateToCategory(filter: string) {
    goto(`${basePath}/config/${filter}`);
  }
</script>

<div class="flex items-center gap-2 mb-6">
  <Settings class="h-5 w-5 text-muted-foreground" />
  <span class="text-sm font-medium text-muted-foreground">
    Parameter Category:
  </span>

  <DropdownMenu>
    <DropdownMenuTrigger asChild>
      <Button variant="outline" class="justify-between min-w-[200px]">
        {currentCategory?.label || "Select Category"}
        <ChevronDown class="ml-2 h-4 w-4" />
      </Button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" class="w-80">
      {#each parameterCategories as category}
        <DropdownMenuItem on:click={() => navigateToCategory(category.filter)}>
          <div class="flex flex-col items-start p-3 cursor-pointer">
            <div class="font-medium">{category.label}</div>
            <div class="text-xs text-muted-foreground">
              {category.description}
            </div>
          </div>
        </DropdownMenuItem>
      {/each}
    </DropdownMenuContent>
  </DropdownMenu>
</div>
