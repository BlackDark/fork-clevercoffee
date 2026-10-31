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
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Ota from "../components/Ota.svelte";
  const m = useMachine();
  let action = $state("");
  let busy = $state(false);
  let file = $state<File | null>(null);
  const descriptions: Record<string, string> = {
    restart: "Restart the device? The browser may disconnect briefly.",
    "factory-reset":
      "Erase all saved settings and restart? This cannot be undone.",
    "wifi-reset":
      "Erase saved Wi-Fi credentials and restart? You will need to reconnect the machine.",
    upload:
      "Restore this configuration? Settings will change and the device may restart.",
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
        await m.parameters.refetch();
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
    <h1>System</h1>
    <p>Back up your settings, update your device, and keep things running.</p>
  </div>
</div>

<div class="two-columns">
  <Card.Root class="panel"
    ><Card.Content>
      <div class="panel-heading">
        <div>
          <Card.Title
            role="heading"
            aria-level={2}>Power & standby</Card.Title
          >
          <p>Wake, sleep, or restart the device.</p>
        </div>
        <Power size={20} />
      </div>
      <div class="flex flex-wrap gap-2">
        <Button
          type="button"
          variant="success"
          disabled={m.command.isPending}
          onclick={() => m.run("/wake")}><Power size={16} />Wake up</Button
        ><Button
          type="button"
          variant="outline"
          disabled={m.command.isPending}
          onclick={() => m.run("/sleep")}><Moon size={16} />Sleep</Button
        ><Button
          type="button"
          variant="warning"
          onclick={() => ask("restart")}><RotateCcw size={16} />Restart</Button
        >
      </div>
    </Card.Content></Card.Root
  >
  <Card.Root class="panel"
    ><Card.Content>
      <div class="panel-heading">
        <div>
          <Card.Title
            role="heading"
            aria-level={2}>Reset options</Card.Title
          >
          <p>These actions erase saved settings.</p>
        </div>
        <TriangleAlert size={20} />
      </div>
      <div class="flex flex-wrap gap-2">
        <Button
          type="button"
          variant="warning"
          onclick={() => ask("wifi-reset")}
          ><Wifi size={16} />Reset Wi-Fi</Button
        ><Button
          type="button"
          variant="destructive"
          onclick={() => ask("factory-reset")}
          ><TriangleAlert size={16} />Factory reset</Button
        >
      </div>
    </Card.Content></Card.Root
  >
  <Card.Root class="panel"
    ><Card.Content>
      <div class="panel-heading">
        <div>
          <Card.Title
            role="heading"
            aria-level={2}>Configuration backup</Card.Title
          >
          <p>Download the current configuration.</p>
        </div>
        <Download size={20} />
      </div>
      <Button
        variant="success"
        href={`${API_BASE_URL}/config/download`}
        download="clevercoffee-config.json"
        ><Download size={16} />Download configuration</Button
      >
    </Card.Content></Card.Root
  >
  <Card.Root class="panel"
    ><Card.Content>
      <div class="panel-heading">
        <div>
          <Card.Title
            role="heading"
            aria-level={2}>Restore configuration</Card.Title
          >
          <p>Restore settings from a JSON backup (up to 8 KB).</p>
        </div>
        <Upload size={20} />
      </div>
      <Label
        class="sr-only"
        for="config-file">Configuration file</Label
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
        ><Upload size={16} />Restore configuration</Button
      >
    </Card.Content></Card.Root
  >
</div>
<Ota />
<ConfirmDialog
  bind:open
  title={action === "upload"
    ? "Restore configuration?"
    : "Confirm device action"}
  description={descriptions[action] ?? ""}
  destructive={action.includes("reset")}
  confirm={perform}
  {busy}
  label="Continue"
/>
