<script lang="ts">
  import {
    Power,
    Moon,
    RotateCcw,
    Download,
    Upload,
    TriangleAlert,
  } from "@lucide/svelte";
  import { useMachine } from "@/lib/machine.svelte";
  import { apiFetch, API_BASE_URL } from "@/lib/api-config";
  import { ConfigUploadResponseSchema } from "@/lib/schemas";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Ota from "../components/Ota.svelte";
  const m = useMachine();
  let action = $state("");
  let busy = $state(false);
  let message = $state("");
  let failed = $state(false);
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
    message = "";
    failed = false;
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
        message =
          result.message || "Configuration restored. The device may restart.";
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
        message =
          "Request accepted. The device may disconnect while restarting.";
      }
    } catch (e) {
      failed = true;
      message = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
      open = false;
    }
  }
  async function standby(route: string) {
    try {
      await m.command.mutateAsync(route);
      message = "Device updated";
      failed = false;
    } catch (e) {
      failed = true;
      message = String(e);
    }
  }
</script>

<div class="page-heading">
  <div>
    <h1>System</h1>
    <p>Back up your settings, update your device, and keep things running.</p>
  </div>
</div>
{#if message}<div
    class="notice"
    class:error={failed}
    role={failed ? "alert" : "status"}
  >
    {message}
  </div>{/if}
<div class="two-columns">
  <section class="panel">
    <div class="panel-heading">
      <div>
        <h2>Power & standby</h2>
        <p>Wake, sleep, or restart the device.</p>
      </div>
      <Power size={20} />
    </div>
    <div class="flex flex-wrap gap-2">
      <button
        class="btn"
        disabled={m.command.isPending}
        onclick={() => standby("/wake")}><Power size={16} />Wake up</button
      ><button
        class="btn secondary"
        disabled={m.command.isPending}
        onclick={() => standby("/sleep")}><Moon size={16} />Sleep</button
      ><button
        class="btn secondary"
        onclick={() => ask("restart")}><RotateCcw size={16} />Restart</button
      >
    </div>
  </section>
  <section class="panel">
    <div class="panel-heading">
      <div>
        <h2>Reset options</h2>
        <p>These actions erase saved settings.</p>
      </div>
      <TriangleAlert size={20} />
    </div>
    <div class="flex flex-wrap gap-2">
      <button
        class="btn secondary"
        onclick={() => ask("wifi-reset")}>Reset Wi-Fi</button
      ><button
        class="btn danger"
        onclick={() => ask("factory-reset")}>Factory reset</button
      >
    </div>
  </section>
  <section class="panel">
    <div class="panel-heading">
      <div>
        <h2>Configuration backup</h2>
        <p>Download the current configuration.</p>
      </div>
      <Download size={20} />
    </div>
    <a
      class="btn secondary"
      href={`${API_BASE_URL}/config/download`}
      download="clevercoffee-config.json"
      ><Download size={16} />Download configuration</a
    >
  </section>
  <section class="panel">
    <div class="panel-heading">
      <div>
        <h2>Restore configuration</h2>
        <p>Restore settings from a JSON backup (up to 50 KB).</p>
      </div>
      <Upload size={20} />
    </div>
    <label
      class="sr-only"
      for="config-file">Configuration file</label
    ><input
      class="input mb-4"
      id="config-file"
      type="file"
      accept=".json"
      disabled={busy}
      onchange={(e) => {
        file = e.currentTarget.files?.[0] ?? null;
      }}
    /><button
      class="btn secondary"
      onclick={() => ask("upload")}
      disabled={!file || busy}>Restore configuration</button
    >
  </section>
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
