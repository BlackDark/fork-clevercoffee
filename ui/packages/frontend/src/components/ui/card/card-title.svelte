<script lang="ts">
  import { cn, type WithElementRef } from "@/lib/utils.js";
  import type { HTMLAttributes } from "svelte/elements";

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLDivElement>> = $props();
</script>

<div
  bind:this={ref}
  data-slot="card-title"
  class={cn(
    "flex items-center gap-3 text-base leading-snug font-medium md:text-lg group-data-[size=sm]/card:text-sm",
    className,
  )}
  {...restProps}
>
  {@render children?.()}
</div>
