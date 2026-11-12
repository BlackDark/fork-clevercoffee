<script lang="ts">
  import * as Dialog from "./ui/dialog";
  import { Button } from "./ui/button";
  import { X, Check } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import * as m from "@/paraglide/messages.js";
  let {
    open = $bindable(false),
    title,
    description,
    children,
    confirm,
    busy = false,
    label = m.confirm_label(),
    destructive = false,
  }: {
    open?: boolean;
    title: string;
    description: string;
    children?: Snippet;
    confirm: () => void;
    busy?: boolean;
    label?: string;
    destructive?: boolean;
  } = $props();
</script>

<Dialog.Root bind:open>
  <Dialog.Content
    showCloseButton={!busy}
    class="max-h-[90vh] overflow-y-auto sm:max-w-lg"
    onEscapeKeydown={(event) => {
      if (busy) event.preventDefault();
    }}
    onInteractOutside={(event) => {
      if (busy) event.preventDefault();
    }}
  >
    <Dialog.Header
      ><Dialog.Title>{title}</Dialog.Title><Dialog.Description
        >{description}</Dialog.Description
      ></Dialog.Header
    >
    {#if children}<div class="max-h-72 overflow-auto">
        {@render children()}
      </div>{/if}
    <Dialog.Footer>
      <Button
        variant="outline"
        disabled={busy}
        onclick={() => {
          open = false;
        }}><X size={16} />{m.confirm_cancel()}</Button
      >
      <Button
        variant={destructive ? "destructive" : "default"}
        disabled={busy}
        onclick={confirm}
        ><Check size={16} />{busy ? m.confirm_working() : label}</Button
      >
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
