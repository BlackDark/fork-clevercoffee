<script lang="ts">
  import { onMount } from "svelte";
  import * as Alert from "@/components/ui/alert";
  import * as Card from "@/components/ui/card";
  import { Skeleton } from "@/components/ui/skeleton";
  import { Activity, AlertTriangle, Info } from "@lucide/svelte";
  import { API_BASE_URL, SERVER_BASE_URL } from "@/lib/api-config";
  import {
    RECONNECT_MS,
    formatKiB,
    formatMicros,
    formatStack,
    formatUptime,
    saveResultName,
  } from "@/lib/diagnostics";
  import { API_ROUTES } from "@/lib/routes";
  import { MetricsSchema, type Metrics } from "@/lib/schemas";

  let m = $state<Metrics | null>(null);
  let stream = $state<"connecting" | "reconnecting" | "live" | "invalid">(
    "connecting",
  );

  // The device pushes metrics each second. A refused stream (503: busy or low
  // memory) is not retried by the browser, so reopen it by hand.
  onMount(() => {
    let source: EventSource | undefined;
    let retry: ReturnType<typeof setTimeout> | undefined;
    const connect = () => {
      const events = new EventSource(`${SERVER_BASE_URL}/events?metrics=1`);
      source = events;
      events.onopen = () => {
        stream = "live";
      };
      events.onerror = () => {
        // The browser retries a dropped stream itself; only a refused one (CLOSED) is ours.
        if (events.readyState === EventSource.CLOSED) {
          stream = "reconnecting";
          clearTimeout(retry);
          retry = setTimeout(connect, RECONNECT_MS);
        }
      };
      events.addEventListener("metrics", (event) => {
        let payload: unknown;
        try {
          payload = JSON.parse(event.data);
        } catch {
          stream = "invalid";
          return;
        }
        const parsed = MetricsSchema.safeParse(payload);
        if (!parsed.success) {
          stream = "invalid";
          return;
        }
        m = parsed.data;
        stream = "live";
      });
    };
    connect();
    return () => {
      clearTimeout(retry);
      source?.close();
    };
  });
</script>

{#snippet row(label: string, value: string)}
  <div class="flex justify-between gap-4 py-1 text-sm">
    <dt class="text-muted-foreground">{label}</dt>
    <dd class="text-right font-mono">{value}</dd>
  </div>
{/snippet}

<div class="page-heading">
  <h1>Diagnostics</h1>
  <p>Controller health since boot, pushed every second while open.</p>
</div>

{#if stream === "reconnecting"}
  <Alert.Root
    role="status"
    variant="warning"
    class="mb-4"
    ><AlertTriangle size={18} /><Alert.Description class="text-inherit"
      >No stream from the device. Retrying every {RECONNECT_MS / 1000} s.</Alert.Description
    ></Alert.Root
  >
{:else if stream === "invalid"}
  <Alert.Root
    role="alert"
    variant="destructive"
    class="mb-4"
    ><Info size={18} /><Alert.Description
      >Invalid metrics from the device.</Alert.Description
    ></Alert.Root
  >
{/if}

{#if m}
  <div class="grid gap-4 md:grid-cols-2">
    <Card.Root class="min-w-0">
      <Card.Header>
        <Card.Title
          role="heading"
          aria-level={2}>Device</Card.Title
        >
        <Card.Description>Firmware, boot and link state.</Card.Description>
        <Card.Action class="text-muted-foreground"
          ><Activity size={20} /></Card.Action
        >
      </Card.Header>
      <Card.Content>
        <dl>
          {@render row("Firmware", m.firmwareVersion)}
          {@render row("Uptime", formatUptime(m.uptime))}
          {@render row("Reset reason (esp-hal code)", String(m.resetReason))}
          {@render row(
            "WiFi",
            m.wifiConnected ? `connected, ${m.wifiRssi} dBm` : "down",
          )}
          {@render row("WiFi reconnects", String(m.wifiReconnects))}
          {@render row("MQTT", m.mqttConnected ? "connected" : "not connected")}
        </dl>
      </Card.Content>
    </Card.Root>

    <Card.Root class="min-w-0">
      <Card.Header>
        <Card.Title
          role="heading"
          aria-level={2}>Memory</Card.Title
        >
        <Card.Description
          >Heap and stack; refusals when heap runs low.</Card.Description
        >
      </Card.Header>
      <Card.Content>
        <dl>
          {@render row("Free heap", formatKiB(m.freeHeap))}
          {@render row("Lowest free heap", formatKiB(m.minFreeHeap))}
          {@render row("Lowest free stack", formatStack(m.minFreeStack))}
          {@render row("Low-heap refusals (503)", String(m.lowHeapRefusals))}
        </dl>
      </Card.Content>
    </Card.Root>

    <Card.Root class="min-w-0">
      <Card.Header>
        <Card.Title
          role="heading"
          aria-level={2}>Control loop</Card.Title
        >
        <Card.Description
          >Tick timing; a late tick is a gap over 20 ms.</Card.Description
        >
      </Card.Header>
      <Card.Content>
        <dl>
          {@render row("Longest gap", formatMicros(m.loopGapMaxUs))}
          {@render row("Slowest tick", formatMicros(m.loopTickMaxUs))}
          {@render row("Late ticks", String(m.loopLateTicks))}
        </dl>
      </Card.Content>
    </Card.Root>

    <Card.Root class="min-w-0">
      <Card.Header>
        <Card.Title
          role="heading"
          aria-level={2}>Sensors</Card.Title
        >
        <Card.Description>Frames and read errors since boot.</Card.Description>
      </Card.Header>
      <Card.Content>
        <dl>
          {@render row("TSIC good frames", String(m.tsicGoodFrames))}
          {@render row("TSIC bad frames", String(m.tsicBadFrames))}
          {@render row(
            "TSIC last good (uptime)",
            m.tsicLastGoodMs ? `${formatUptime(m.tsicLastGoodMs)}` : "never",
          )}
          {@render row("DS18B20 read errors", String(m.ds18b20ReadErrors))}
        </dl>
      </Card.Content>
    </Card.Root>

    <Card.Root class="min-w-0">
      <Card.Header>
        <Card.Title
          role="heading"
          aria-level={2}>Settings and log</Card.Title
        >
        <Card.Description>Saves to flash and the log ring.</Card.Description>
      </Card.Header>
      <Card.Content>
        <dl>
          {@render row("Saves", String(m.settingsSaves))}
          {@render row("Save failures", String(m.settingsSaveFailures))}
          {@render row("Last save", saveResultName(m.settingsLastSave))}
          {@render row("Log bytes", String(m.logBytes))}
        </dl>
        <a
          class="mt-3 inline-block text-sm underline"
          href={`${API_BASE_URL}${API_ROUTES.LOGS}`}>Open the log</a
        >
      </Card.Content>
    </Card.Root>
  </div>
{:else}
  <div
    role="status"
    aria-label="Loading diagnostics"
    class="grid gap-5 md:grid-cols-2"
  >
    <Skeleton class="h-48 rounded-xl" /><Skeleton class="h-48 rounded-xl" />
  </div>
{/if}
