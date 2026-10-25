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
  import { apiFetch, API_BASE_URL } from "@/lib/api-config";
  import { ConfigUploadResponseSchema } from "@/lib/schemas";
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
        if (
          !file ||
          file.size === 0 ||
          file.size > 50 * 1024 ||
          !file.name.toLowerCase().endsWith(".json")
        )
          throw new Error("Choose a nonempty JSON file up to 50 KB.");
        const content = await file.text();
        const parsed: unknown = JSON.parse(content);
        if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
          throw new Error("Configuration must be a JSON object.");
        const response = await apiFetch("/config/upload", {
          method: "POST",
          body: content,
          signal: AbortSignal.timeout(30000),
        });
        const result = ConfigUploadResponseSchema.parse(await response.json());
        if (!response.ok || !result.success)
          throw new Error(result.message || "Configuration upload failed");
        toast.success(
          result.message || "Configuration restored. The device may restart.",
        );
        await m.parameters.refetch();
      } else {
        const response = await apiFetch(`/${action}`, {
          method: "POST",
          signal: AbortSignal.timeout(15000),
        });
        const text = await response.text();
        if (!response.ok) throw new Error(`HTTP ${response.status}: ${text}`);
        let result: unknown;
        try {
          result = JSON.parse(text);
        } catch {
          result = null;
        }
        if (
          result &&
          typeof result === "object" &&
          "success" in result &&
          result.success === false
        )
          throw new Error("Device refused the action");
        toast.success(
          "Request accepted. The device may disconnect while restarting.",
        );
      }
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      busy = false;
      open = false;
    }
  }
  async function standby(route: string) {
    try {
      await m.command.mutateAsync(route);
      toast.success("Device updated");
    } catch (e) {
      toast.error(String(e));
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
          variant="default"
          class="bg-green-700 text-white hover:bg-green-800"
          disabled={m.command.isPending}
          onclick={() => standby("/wake")}><Power size={16} />Wake up</Button
        ><Button
          type="button"
          variant="outline"
          disabled={m.command.isPending}
          onclick={() => standby("/sleep")}><Moon size={16} />Sleep</Button
        ><Button
          type="button"
          variant="default"
          class="bg-orange-700 text-white hover:bg-orange-800"
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
          variant="default"
          class="bg-orange-700 text-white hover:bg-orange-800"
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
        variant="default"
        class="bg-green-700 text-white hover:bg-green-800"
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
          <p>Restore settings from a JSON backup (up to 50 KB).</p>
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
        variant="default"
        class="bg-blue-700 text-white hover:bg-blue-800"
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
