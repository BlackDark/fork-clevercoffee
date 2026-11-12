<script lang="ts">
  import * as m from "@/paraglide/messages.js";
  import * as Tooltip from "./ui/tooltip";
  import { Button } from "./ui/button";
  import { Info } from "@lucide/svelte";
  let {
    id,
    text,
    unavailable = false,
  }: { id: string; text: string; unavailable?: boolean } = $props();
</script>

<Tooltip.Root>
  <span
    class="parameter-hint"
    class:unavailable
  >
    <Tooltip.Trigger
      type="button"
      aria-label={unavailable ? m.param_disabled() : m.param_help()}
    >
      {#snippet child({ props })}
        <Button
          {...props}
          variant={unavailable ? "outline" : "ghost"}
          size={unavailable ? "default" : "icon-sm"}
          class={unavailable
            ? "border-blue-200 bg-blue-50 text-blue-600 dark:border-blue-800 dark:bg-blue-950 dark:text-blue-300"
            : "text-muted-foreground"}
        >
          <Info size={16} />{#if unavailable}<span
              >{m.param_disabled_label()}</span
            >{/if}
        </Button>
      {/snippet}
    </Tooltip.Trigger>
  </span>
  <Tooltip.Content
    role="tooltip"
    class="max-w-72 text-pretty"
    {id}
    sideOffset={6}
    collisionPadding={16}
  >
    {#if unavailable}<strong>{m.param_disabled()}</strong>{/if}{text}
  </Tooltip.Content>
</Tooltip.Root>
