<script lang="ts">
  import * as Alert from "@/components/ui/alert";
  import { Button } from "@/components/ui/button";
  import { Input } from "@/components/ui/input";
  import { Label } from "@/components/ui/label";
  import * as Card from "@/components/ui/card";
  import { Progress } from "@/components/ui/progress";
  import SelectField from "./SelectField.svelte";
  import { toast } from "svelte-sonner";
  import { createMutation, createQuery } from "@tanstack/svelte-query";
  import {
    Upload,
    Link,
    HardDrive,
    TriangleAlert,
    RefreshCw,
  } from "@lucide/svelte";
  import { apiJsonFetch } from "@/lib/api-config";
  import { ApiResponseSchema, OtaStatusSchema } from "@/lib/schemas";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  let type = $state<"firmware" | "filesystem">("firmware");
  let file = $state<File | null>(null);
  let fileInput = $state<HTMLInputElement | null>(null);
  let url = $state("");
  let open = $state(false);
  let source = $state<"file" | "url">("file");
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
      toast.success(
        result.message ||
          "Update accepted. Watch device status for completion.",
      );
      await status.refetch();
    },
  }));
  const busy = $derived(upload.isPending || !!status.data?.updateInProgress);
  async function run() {
    try {
      await upload.mutateAsync();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      open = false;
    }
  }
  function ask(next: "file" | "url") {
    source = next;
    open = true;
  }
</script>

<Card.Root class="panel mt-6"
  ><Card.Content>
    <div class="panel-heading">
      <div>
        <Card.Title
          role="heading"
          aria-level={2}>Device updates</Card.Title
        >
        <p>
          Install firmware or filesystem updates. The device will restart after
          a successful update.
        </p>
      </div>
      <HardDrive size={20} />
    </div>
    <Alert.Root
      class="mb-4 border-amber-300 bg-amber-50 text-amber-950 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-100"
      ><TriangleAlert size={18} /><Alert.Description class="text-inherit">
        Keep the device powered throughout the update. Updates may be refused
        while brewing or steaming.
      </Alert.Description></Alert.Root
    >
    <Label
      for="update-type"
      class="font-medium">Update type</Label
    >
    <div class="my-3 max-w-sm">
      <SelectField
        id="update-type"
        label="Update type"
        value={type}
        options={[
          { value: "firmware", label: "Firmware (.bin)" },
          { value: "filesystem", label: "Filesystem (.bin / .img)" },
        ]}
        disabled={busy}
        onchange={(value) => {
          type = value as typeof type;
          file = null;
          if (fileInput) fileInput.value = "";
        }}
      />
    </div>
    <div class="two-columns">
      <div>
        <Label
          for="ota-file"
          class="font-medium flex gap-2 items-center mb-3"
          ><Upload size={17} />Upload a file</Label
        ><Input
          bind:ref={fileInput}
          id="ota-file"
          type="file"
          class="mb-3"
          accept={type === "firmware" ? ".bin" : ".bin,.img"}
          disabled={busy}
          onchange={(e) => (file = e.currentTarget.files?.[0] ?? null)}
        /><Button
          type="button"
          variant="default"
          disabled={!file || busy || status.isError || status.isPending}
          onclick={() => ask("file")}
          ><Upload size={16} />{upload.isPending
            ? "Updating…"
            : "Upload update"}</Button
        >
      </div>
      <div>
        <Label
          for="ota-url"
          class="font-medium flex gap-2 items-center mb-3"
          ><Link size={17} />Update from URL</Label
        >
        <form
          onsubmit={(e) => {
            e.preventDefault();
            ask("url");
          }}
        >
          <Input
            id="ota-url"
            class="mb-3"
            type="url"
            required
            placeholder="https://example.com/firmware.bin"
            bind:value={url}
            disabled={busy}
          /><Button
            type="submit"
            variant="outline"
            disabled={!url || busy || status.isError || status.isPending}
            ><Link size={16} />Update from URL</Button
          >
        </form>
        <p class="text-xs text-muted-foreground mt-2">
          Availability depends on firmware support. Unsupported requests are
          reported by the device.
        </p>
      </div>
    </div>
    {#if status.isError}<Alert.Root
        role="alert"
        variant="destructive"
        class="mb-4"
        ><Alert.Description>
          Update status unavailable. <Button
            type="button"
            variant="link"
            class="h-auto p-0"
            onclick={() => status.refetch()}
            ><RefreshCw size={16} />Retry</Button
          >
        </Alert.Description></Alert.Root
      >{/if}
    {#if busy || (status.data?.status !== "idle" && status.data)}<div
        class="mt-5"
        role="status"
      >
        <p>{status.data?.message || status.data?.status || "Uploading…"}</p>
        <Progress
          max={100}
          value={status.data?.progress ?? 0}
          aria-label="Device update progress"
        ></Progress>{#if status.data?.error}<p class="text-red-600">
            {status.data.error}
          </p>{/if}
      </div>{/if}
  </Card.Content></Card.Root
>
<ConfirmDialog
  bind:open
  title={`Install ${type} update?`}
  description={`The device will install ${source === "file" ? file?.name : url} and restart. Do not disconnect power.`}
  confirm={run}
  busy={upload.isPending}
  label="Install update"
/>
