<script lang="ts">
  import * as Tooltip from "./components/ui/tooltip";
  import { QueryClient, QueryClientProvider } from "@tanstack/svelte-query";
  import Shell from "./components/Shell.svelte";
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: 1, staleTime: 5000, refetchOnWindowFocus: false },
      mutations: { retry: false },
    },
  });
</script>

<QueryClientProvider {client}
  ><Tooltip.Provider delayDuration={150}><Shell /></Tooltip.Provider
  ></QueryClientProvider
>
