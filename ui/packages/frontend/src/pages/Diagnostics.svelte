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
  import * as m from "@/paraglide/messages.js";

  let metrics = $state<Metrics | null>(null);
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
        metrics = parsed.data;
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
  <h1>{m.diag_title()}</h1>
  <p>{m.diag_subtitle()}</p>
</div>

{#if stream === "reconnecting"}
  <Alert.Root
    role="status"
    variant="warning"
    class="mb-4"
    ><AlertTriangle size={18} /><Alert.Description class="text-inherit"
      >{m.diag_stream_retrying({
        seconds: RECONNECT_MS / 1000,
      })}</Alert.Description
    ></Alert.Root
  >
{:else if stream === "invalid"}
  <Alert.Root
    role="alert"
    variant="destructive"
    class="mb-4"
    ><Info size={18} /><Alert.Description
      >{m.diag_invalid_metrics()}</Alert.Description
    ></Alert.Root
  >
{/if}

{#if metrics}
  <div class="grid gap-4 md:grid-cols-2">
    <Card.Root class="min-w-0">
      <Card.Header>
        <Card.Title
          role="heading"
          aria-level={2}>{m.diag_device()}</Card.Title
        >
        <Card.Description>{m.diag_device_description()}</Card.Description>
        <Card.Action class="text-muted-foreground"
          ><Activity size={20} /></Card.Action
        >
      </Card.Header>
      <Card.Content>
        <dl>
          {@render row(m.diag_firmware(), metrics.firmwareVersion)}
          {@render row(m.diag_uptime(), formatUptime(metrics.uptime))}
          {@render row(m.diag_reset_reason(), String(metrics.resetReason))}
          {@render row(
            m.diag_wifi(),
            metrics.wifiConnected
              ? m.diag_wifi_connected({ rssi: metrics.wifiRssi })
              : m.diag_wifi_down(),
          )}
          {@render row(
            m.diag_wifi_reconnects(),
            String(metrics.wifiReconnects),
          )}
          {@render row(
            m.diag_mqtt(),
            metrics.mqttConnected ? m.diag_connected() : m.diag_not_connected(),
          )}
        </dl>
      </Card.Content>
    </Card.Root>

    <Card.Root class="min-w-0">
      <Card.Header>
        <Card.Title
          role="heading"
          aria-level={2}>{m.diag_memory()}</Card.Title
        >
        <Card.Description>{m.diag_memory_description()}</Card.Description>
      </Card.Header>
      <Card.Content>
        <dl>
          {@render row(m.diag_free_heap(), formatKiB(metrics.freeHeap))}
          {@render row(
            m.diag_lowest_free_heap(),
            formatKiB(metrics.minFreeHeap),
          )}
          {@render row(
            m.diag_lowest_free_stack(),
            formatStack(metrics.minFreeStack),
          )}
          {@render row(
            m.diag_low_heap_refusals(),
            String(metrics.lowHeapRefusals),
          )}
        </dl>
      </Card.Content>
    </Card.Root>

    <Card.Root class="min-w-0">
      <Card.Header>
        <Card.Title
          role="heading"
          aria-level={2}>{m.diag_control_loop()}</Card.Title
        >
        <Card.Description>{m.diag_control_loop_description()}</Card.Description>
      </Card.Header>
      <Card.Content>
        <dl>
          {@render row(
            m.diag_longest_gap(),
            formatMicros(metrics.loopGapMaxUs),
          )}
          {@render row(
            m.diag_slowest_tick(),
            formatMicros(metrics.loopTickMaxUs),
          )}
          {@render row(m.diag_late_ticks(), String(metrics.loopLateTicks))}
        </dl>
      </Card.Content>
    </Card.Root>

    <Card.Root class="min-w-0">
      <Card.Header>
        <Card.Title
          role="heading"
          aria-level={2}>{m.diag_sensors()}</Card.Title
        >
        <Card.Description>{m.diag_sensors_description()}</Card.Description>
      </Card.Header>
      <Card.Content>
        <dl>
          {@render row(
            m.diag_tsic_good_frames(),
            String(metrics.tsicGoodFrames),
          )}
          {@render row(m.diag_tsic_bad_frames(), String(metrics.tsicBadFrames))}
          {@render row(
            m.diag_tsic_last_good(),
            metrics.tsicLastGoodMs
              ? `${formatUptime(metrics.tsicLastGoodMs)}`
              : m.diag_never(),
          )}
          {@render row(
            m.diag_ds18b20_read_errors(),
            String(metrics.ds18b20ReadErrors),
          )}
        </dl>
      </Card.Content>
    </Card.Root>

    <Card.Root class="min-w-0">
      <Card.Header>
        <Card.Title
          role="heading"
          aria-level={2}>{m.diag_settings_and_log()}</Card.Title
        >
        <Card.Description>{m.diag_settings_description()}</Card.Description>
      </Card.Header>
      <Card.Content>
        <dl>
          {@render row(m.diag_saves(), String(metrics.settingsSaves))}
          {@render row(
            m.diag_save_failures(),
            String(metrics.settingsSaveFailures),
          )}
          {@render row(
            m.diag_last_save(),
            saveResultName(metrics.settingsLastSave),
          )}
          {@render row(m.diag_log_bytes(), String(metrics.logBytes))}
        </dl>
        <a
          class="mt-3 inline-block text-sm underline"
          href={`${API_BASE_URL}${API_ROUTES.LOGS}`}>{m.diag_open_the_log()}</a
        >
      </Card.Content>
    </Card.Root>
  </div>
{:else}
  <div
    role="status"
    aria-label={m.diag_loading()}
    class="grid gap-5 md:grid-cols-2"
  >
    <Skeleton class="h-48 rounded-xl" /><Skeleton class="h-48 rounded-xl" />
  </div>
{/if}
