<script lang="ts">
  import { Dialog } from "bits-ui";
  import type { Snippet } from "svelte";
  let {
    open = $bindable(false),
    title,
    description,
    children,
    confirm,
    busy = false,
    label = "Confirm",
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
  <Dialog.Portal
    ><Dialog.Overlay class="dialog-overlay" /><Dialog.Content
      class="dialog-content"
      onEscapeKeydown={(e) => {
        if (busy) e.preventDefault();
      }}
      onInteractOutside={(e) => {
        if (busy) e.preventDefault();
      }}
    >
      <Dialog.Title class="text-xl font-semibold">{title}</Dialog.Title>
      <Dialog.Description class="mt-2 text-muted-foreground"
        >{description}</Dialog.Description
      >
      {#if children}<div class="my-5 max-h-72 overflow-auto">
          {@render children()}
        </div>{/if}
      <div class="mt-6 flex justify-end gap-3">
        <Dialog.Close
          class="btn secondary"
          disabled={busy}>Cancel</Dialog.Close
        ><button
          class:danger={destructive}
          class="btn"
          disabled={busy}
          onclick={confirm}>{busy ? "Working…" : label}</button
        >
      </div>
    </Dialog.Content></Dialog.Portal
  >
</Dialog.Root>
