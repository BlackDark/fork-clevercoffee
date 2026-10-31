<script lang="ts">
  import * as Alert from "@/components/ui/alert";
  import { Button } from "@/components/ui/button";
  import { Input } from "@/components/ui/input";
  import { Label } from "@/components/ui/label";
  import * as Card from "@/components/ui/card";
  import { Progress } from "@/components/ui/progress";
  import { toast } from "svelte-sonner";
  import { createMutation, createQuery } from "@tanstack/svelte-query";
  import { Upload, HardDrive, TriangleAlert, RefreshCw } from "@lucide/svelte";
  import { apiJsonFetch } from "@/lib/api-config";
  import { ApiResponseSchema, OtaStatusSchema } from "@/lib/schemas";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  let file = $state<File | null>(null);
  let open = $state(false);
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
      if (!file || file.size === 0 || !/\.bin$/i.test(file.name))
        throw new Error("Select a nonempty .bin firmware file.");
      const body = new FormData();
      body.append("firmware", file);
      const result = ApiResponseSchema.parse(
        await apiJsonFetch("/ota/firmware", { method: "POST", body }, 180000),
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
</script>

<Card.Root class="panel mt-6"
  ><Card.Content>
    <div class="panel-heading">
      <div>
        <Card.Title
          role="heading"
          aria-level={2}>Firmware update</Card.Title
        >
        <p>
          The firmware image includes this web interface. The device will
          restart after a successful update.
        </p>
      </div>
      <HardDrive size={20} />
    </div>
    <Alert.Root
      variant="warning"
      class="mb-4"
      ><TriangleAlert size={18} /><Alert.Description class="text-inherit">
        Keep the device powered throughout the update. Updates may be refused
        while brewing or steaming.
      </Alert.Description></Alert.Root
    >
    <div class="max-w-sm">
      <Label
        for="ota-file"
        class="font-medium flex gap-2 items-center mb-3"
        ><Upload size={17} />Firmware file (.bin)</Label
      ><Input
        id="ota-file"
        type="file"
        class="mb-3"
        accept=".bin"
        disabled={busy}
        onchange={(e) => (file = e.currentTarget.files?.[0] ?? null)}
      /><Button
        type="button"
        variant="default"
        disabled={!file || busy || status.isError || status.isPending}
        onclick={() => (open = true)}
        ><Upload size={16} />{upload.isPending
          ? "Updating…"
          : "Upload update"}</Button
      >
    </div>
    {#if status.isError}<Alert.Root
        role="alert"
        variant="destructive"
        class="mb-4"
        ><Alert.Description>
          Update status unavailable.
        </Alert.Description><Alert.Action
          ><Button
            type="button"
            variant="outline"
            size="sm"
            onclick={() => status.refetch()}
            ><RefreshCw size={16} />Retry</Button
          ></Alert.Action
        ></Alert.Root
      >{/if}
    {#if busy || (status.data?.status !== "idle" && status.data)}<div
        class="mt-5"
        role="status"
      >
        <p>
          {status.data?.message || status.data?.status || "Uploading…"}
        </p>
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
  title="Install firmware update?"
  description={`The device will install ${file?.name} and restart. Do not disconnect power.`}
  confirm={run}
  busy={upload.isPending}
  label="Install update"
/>
