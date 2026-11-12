<script lang="ts">
  import { Button } from "@/components/ui/button";
  import { Input } from "@/components/ui/input";
  import { Label } from "@/components/ui/label";
  import * as Card from "@/components/ui/card";
  import { toast } from "svelte-sonner";
  import {
    Power,
    Moon,
    RotateCcw,
    Download,
    Upload,
    TriangleAlert,
    Wifi,
  } from "@lucide/svelte";
  import { useMachine } from "@/lib/machine.svelte";
  import { API_BASE_URL } from "@/lib/api-config";
  import { reportError } from "@/lib/notify";
  import {
    postDeviceAction,
    uploadConfig,
    type DeviceAction,
  } from "@/lib/system-actions";
  import * as m from "@/paraglide/messages.js";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Ota from "../components/Ota.svelte";
  const machine = useMachine();
  let action = $state("");
  let busy = $state(false);
  let file = $state<File | null>(null);
  const descriptions: Record<string, string> = {
    restart: m.system_restart_confirm(),
    "factory-reset": m.system_factory_reset_confirm(),
    "wifi-reset": m.system_wifi_reset_confirm(),
    upload: m.system_upload_confirm(),
  };
  let open = $state(false);
  function ask(next: string) {
    action = next;
    open = true;
  }
  async function perform() {
    busy = true;
    try {
      if (action === "upload") {
        toast.success(await uploadConfig(file));
        await machine.parameters.refetch();
      } else {
        toast.success(await postDeviceAction(action as DeviceAction));
      }
    } catch (e) {
      reportError(e);
    } finally {
      busy = false;
      open = false;
    }
  }
</script>

<div class="page-heading">
  <div>
    <h1>{m.system_title()}</h1>
    <p>{m.system_subtitle()}</p>
  </div>
</div>

<div class="two-columns">
  <Card.Root class="min-w-0"
    ><Card.Header
      ><Card.Title
        role="heading"
        aria-level={2}>{m.system_power_standby()}</Card.Title
      ><Card.Description>{m.system_power_standby_desc()}</Card.Description
      ><Card.Action class="text-muted-foreground"
        ><Power size={20} /></Card.Action
      ></Card.Header
    ><Card.Content>
      <div class="flex flex-wrap gap-2">
        <Button
          type="button"
          variant="success"
          disabled={machine.command.isPending}
          onclick={() => machine.run("/wake")}
          ><Power size={16} />{m.system_wake_up()}</Button
        ><Button
          type="button"
          variant="outline"
          disabled={machine.command.isPending}
          onclick={() => machine.run("/sleep")}
          ><Moon size={16} />{m.system_sleep()}</Button
        ><Button
          type="button"
          variant="warning"
          onclick={() => ask("restart")}
          ><RotateCcw size={16} />{m.system_restart()}</Button
        >
      </div>
    </Card.Content></Card.Root
  >
  <Card.Root class="min-w-0"
    ><Card.Header
      ><Card.Title
        role="heading"
        aria-level={2}>{m.system_reset_options()}</Card.Title
      ><Card.Description>{m.system_reset_options_desc()}</Card.Description
      ><Card.Action class="text-muted-foreground"
        ><TriangleAlert size={20} /></Card.Action
      ></Card.Header
    ><Card.Content>
      <div class="flex flex-wrap gap-2">
        <Button
          type="button"
          variant="warning"
          onclick={() => ask("wifi-reset")}
          ><Wifi size={16} />{m.system_reset_wifi()}</Button
        ><Button
          type="button"
          variant="destructive"
          onclick={() => ask("factory-reset")}
          ><TriangleAlert size={16} />{m.system_factory_reset()}</Button
        >
      </div>
    </Card.Content></Card.Root
  >
  <Card.Root class="min-w-0"
    ><Card.Header
      ><Card.Title
        role="heading"
        aria-level={2}>{m.system_config_backup()}</Card.Title
      ><Card.Description>{m.system_config_backup_desc()}</Card.Description
      ><Card.Action class="text-muted-foreground"
        ><Download size={20} /></Card.Action
      ></Card.Header
    ><Card.Content>
      <Button
        variant="success"
        href={`${API_BASE_URL}/config/download`}
        download="clevercoffee-config.json"
        ><Download size={16} />{m.system_download_configuration()}</Button
      >
    </Card.Content></Card.Root
  >
  <Card.Root class="min-w-0"
    ><Card.Header
      ><Card.Title
        role="heading"
        aria-level={2}>{m.system_restore_configuration()}</Card.Title
      ><Card.Description
        >{m.system_restore_configuration_desc()}</Card.Description
      ><Card.Action class="text-muted-foreground"
        ><Upload size={20} /></Card.Action
      ></Card.Header
    ><Card.Content>
      <Label
        class="sr-only"
        for="config-file">{m.system_configuration_file()}</Label
      ><Input
        class="mb-4"
        id="config-file"
        type="file"
        accept=".json"
        disabled={busy}
        onchange={(e) => {
          file = e.currentTarget.files?.[0] ?? null;
        }}
      /><Button
        type="button"
        variant="info"
        onclick={() => ask("upload")}
        disabled={!file || busy}
        ><Upload size={16} />{m.system_restore_configuration()}</Button
      >
    </Card.Content></Card.Root
  >
</div>
<Ota />
<ConfirmDialog
  bind:open
  title={action === "upload"
    ? m.system_restore_confirm_title()
    : m.system_confirm_device_action()}
  description={descriptions[action] ?? ""}
  destructive={action.includes("reset")}
  confirm={perform}
  {busy}
  label={m.system_continue()}
/>
