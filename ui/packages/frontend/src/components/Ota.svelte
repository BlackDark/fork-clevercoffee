<script lang="ts">
  import * as Alert from "@/components/ui/alert";
  import { Button } from "@/components/ui/button";
  import { Input } from "@/components/ui/input";
  import { Label } from "@/components/ui/label";
  import * as Card from "@/components/ui/card";
  import { Progress } from "@/components/ui/progress";
  import { toast } from "svelte-sonner";
  import { reportError } from "@/lib/notify";
  import { createMutation, createQuery } from "@tanstack/svelte-query";
  import { Upload, HardDrive, TriangleAlert, RefreshCw } from "@lucide/svelte";
  import { apiJsonFetch } from "@/lib/api-config";
  import { ApiResponseSchema, OtaStatusSchema } from "@/lib/schemas";
  import * as m from "@/paraglide/messages.js";
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
      if (latest.error || !latest.data) throw new Error(m.ota_cannot_verify());
      if (latest.data.updateInProgress)
        throw new Error(m.ota_update_already_running());
      if (!file || file.size === 0 || !/\.bin$/i.test(file.name))
        throw new Error(m.ota_select_bin());
      const body = new FormData();
      body.append("firmware", file);
      const result = ApiResponseSchema.parse(
        await apiJsonFetch("/ota/firmware", { method: "POST", body }, 180000),
      );
      if (!result.success)
        throw new Error(result.message || m.ota_device_refused());
      return result;
    },
    onSuccess: async (result) => {
      toast.success(result.message || m.ota_update_accepted());
      await status.refetch();
    },
  }));
  const busy = $derived(upload.isPending || !!status.data?.updateInProgress);
  async function run() {
    try {
      await upload.mutateAsync();
    } catch (e) {
      reportError(e);
    } finally {
      open = false;
    }
  }
</script>

<Card.Root class="min-w-0 mt-6"
  ><Card.Header
    ><Card.Title
      role="heading"
      aria-level={2}>{m.ota_firmware_update()}</Card.Title
    ><Card.Description>
      {m.ota_firmware_update_desc()}
    </Card.Description><Card.Action class="text-muted-foreground"
      ><HardDrive size={20} /></Card.Action
    ></Card.Header
  ><Card.Content>
    <Alert.Root
      variant="warning"
      class="mb-4"
      ><TriangleAlert size={18} /><Alert.Description class="text-inherit">
        {m.ota_keep_powered()}
      </Alert.Description></Alert.Root
    >
    <div class="max-w-sm">
      <Label
        for="ota-file"
        class="font-medium flex gap-2 items-center mb-3"
        ><Upload size={17} />{m.ota_firmware_file()}</Label
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
          ? m.ota_updating()
          : m.ota_upload_update()}</Button
      >
    </div>
    {#if status.isError}<Alert.Root
        role="alert"
        variant="destructive"
        class="mb-4"
        ><Alert.Description>
          {m.ota_status_unavailable()}
        </Alert.Description><Alert.Action
          ><Button
            type="button"
            variant="outline"
            size="sm"
            onclick={() => status.refetch()}
            ><RefreshCw size={16} />{m.ota_retry()}</Button
          ></Alert.Action
        ></Alert.Root
      >{/if}
    {#if busy || (status.data?.status !== "idle" && status.data)}<div
        class="mt-5"
        role="status"
      >
        <p>
          {status.data?.message || status.data?.status || m.ota_uploading()}
        </p>
        <Progress
          max={100}
          value={status.data?.progress ?? 0}
          aria-label={m.ota_progress()}
        ></Progress>{#if status.data?.error}<p class="text-red-600">
            {status.data.error}
          </p>{/if}
      </div>{/if}
  </Card.Content></Card.Root
>
<ConfirmDialog
  bind:open
  title={m.ota_install_title()}
  description={m.ota_install_description({ name: file?.name ?? "" })}
  confirm={run}
  busy={upload.isPending}
  label={m.ota_install_update()}
/>
