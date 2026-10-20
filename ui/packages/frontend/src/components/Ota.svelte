<script lang="ts">
  import { createMutation, createQuery } from "@tanstack/svelte-query";
  import { Upload, Link, HardDrive } from "@lucide/svelte";
  import { apiJsonFetch } from "@/lib/api-config";
  import { ApiResponseSchema, OtaStatusSchema } from "@/lib/schemas";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  let type = $state<"firmware" | "filesystem">("firmware");
  let file = $state<File | null>(null);
  let url = $state("");
  let open = $state(false);
  let source = $state<"file" | "url">("file");
  let message = $state("");
  const status = createQuery(() => ({
    queryKey: ["ota"],
    queryFn: async ({ signal }) =>
      OtaStatusSchema.parse(await apiJsonFetch("/ota/status", { signal })),
    refetchInterval: 1000,
    retry: false,
  }));
  const upload = createMutation(() => ({
    mutationFn: async () => {
      const latest = await status.refetch();
      if (latest.error || !latest.data)
        throw new Error(
          "Cannot verify device update status. Try again when connected.",
        );
      if (latest.data.updateInProgress)
        throw new Error("An update is already running.");
      let body: FormData | URLSearchParams;
      if (source === "file") {
        if (
          !file ||
          file.size === 0 ||
          !(type === "firmware" ? /\.bin$/i : /\.(bin|img)$/i).test(file.name)
        )
          throw new Error("Select a nonempty file of the correct type.");
        body = new FormData();
        body.append(type, file);
      } else {
        const parsed = new URL(url);
        if (!["http:", "https:"].includes(parsed.protocol))
          throw new Error("Use an HTTP or HTTPS URL.");
        body = new URLSearchParams({ url, type });
      }
      const result = ApiResponseSchema.parse(
        await apiJsonFetch(
          source === "file" ? `/ota/${type}` : "/ota/url",
          {
            method: "POST",
            body,
            headers:
              source === "url"
                ? { "Content-Type": "application/x-www-form-urlencoded" }
                : {},
          },
          180000,
        ),
      );
      if (!result.success)
        throw new Error(result.message || "Device refused the update");
      return result;
    },
    onSuccess: async (result) => {
      message =
        result.message ||
        "Update accepted. Watch device status for completion.";
      await status.refetch();
    },
  }));
  const busy = $derived(upload.isPending || !!status.data?.updateInProgress);
  async function run() {
    message = "";
    try {
      await upload.mutateAsync();
    } catch (e) {
      message = e instanceof Error ? e.message : String(e);
    } finally {
      open = false;
    }
  }
  function ask(next: "file" | "url") {
    source = next;
    open = true;
  }
</script>

<section class="panel mt-6">
  <div class="panel-heading">
    <div>
      <h2>Device updates</h2>
      <p>
        Install firmware or filesystem updates. The device will restart after a
        successful update.
      </p>
    </div>
    <HardDrive size={20} />
  </div>
  <div class="notice warning">
    Keep the device powered throughout the update. Updates may be refused while
    brewing or steaming.
  </div>
  <label
    for="update-type"
    class="font-medium">Update type</label
  ><select
    id="update-type"
    class="input my-3 max-w-sm"
    bind:value={type}
    disabled={busy}
    onchange={() => {
      file = null;
    }}
    ><option value="firmware">Firmware (.bin)</option><option value="filesystem"
      >Filesystem (.bin / .img)</option
    ></select
  >
  <div class="two-columns">
    <div>
      <label
        for="ota-file"
        class="font-medium flex gap-2 items-center mb-3"
        ><Upload size={17} />Upload a file</label
      ><input
        id="ota-file"
        type="file"
        class="input mb-3"
        accept={type === "firmware" ? ".bin" : ".bin,.img"}
        disabled={busy}
        onchange={(e) => (file = e.currentTarget.files?.[0] ?? null)}
      /><button
        class="btn"
        disabled={!file || busy || status.isError || status.isPending}
        onclick={() => ask("file")}
        >{upload.isPending ? "Updating…" : "Upload update"}</button
      >
    </div>
    <div>
      <label
        for="ota-url"
        class="font-medium flex gap-2 items-center mb-3"
        ><Link size={17} />Update from URL</label
      >
      <form
        onsubmit={(e) => {
          e.preventDefault();
          ask("url");
        }}
      >
        <input
          id="ota-url"
          class="input mb-3"
          type="url"
          required
          placeholder="https://example.com/firmware.bin"
          bind:value={url}
          disabled={busy}
        /><button
          class="btn secondary"
          disabled={!url || busy || status.isError || status.isPending}
          >Update from URL</button
        >
      </form>
      <p class="text-xs text-muted-foreground mt-2">
        Availability depends on firmware support. Unsupported requests are
        reported by the device.
      </p>
    </div>
  </div>
  {#if status.isError}<p
      role="alert"
      class="notice error"
    >
      Update status unavailable. <button
        class="text-link"
        onclick={() => status.refetch()}>Retry</button
      >
    </p>{/if}
  {#if busy || (status.data?.status !== "idle" && status.data)}<div
      class="mt-5"
      role="status"
    >
      <p>{status.data?.message || status.data?.status || "Uploading…"}</p>
      <progress
        max="100"
        value={status.data?.progress ?? 0}
        aria-label="Device update progress"
      ></progress>{#if status.data?.error}<p class="text-red-600">
          {status.data.error}
        </p>{/if}
    </div>{/if}
  {#if message}<p
      class="notice"
      role="status"
    >
      {message}
    </p>{/if}
</section>
<ConfirmDialog
  bind:open
  title={`Install ${type} update?`}
  description={`The device will install ${source === "file" ? file?.name : url} and restart. Do not disconnect power.`}
  confirm={run}
  busy={upload.isPending}
  label="Install update"
/>
