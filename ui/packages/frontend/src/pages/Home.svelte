<script lang="ts">
  import * as Alert from "@/components/ui/alert";
  import { Button } from "@/components/ui/button";
  import { Input } from "@/components/ui/input";
  import { Label } from "@/components/ui/label";
  import { Badge } from "@/components/ui/badge";
  import * as Card from "@/components/ui/card";
  import { Progress } from "@/components/ui/progress";
  import SwitchField from "../components/SwitchField.svelte";
  import { toast } from "svelte-sonner";
  import { reportError } from "@/lib/notify";
  import {
    Activity,
    Thermometer,
    Zap,
    Droplets,
    Power,
    RotateCcw,
  } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { createQuery } from "@tanstack/svelte-query";
  import { apiJsonFetch, SERVER_BASE_URL } from "@/lib/api-config";
  import { useMachine } from "@/lib/machine.svelte";
  import { HistoryDataSchema, TemperatureDataSchema } from "@/lib/schemas";
  import {
    appendSample,
    historyToChart,
    type ChartData,
  } from "@/lib/telemetry";
  import Chart from "../components/Chart.svelte";
  import * as m from "@/paraglide/messages.js";
  let { dark = false }: { dark?: boolean } = $props();
  const machine = useMachine();
  const history = createQuery(() => ({
    queryKey: ["history"],
    queryFn: async ({ signal }) =>
      HistoryDataSchema.parse(await apiJsonFetch("/history", { signal })),
    staleTime: 0,
  }));
  const initial = createQuery(() => ({
    queryKey: ["temperature"],
    queryFn: async ({ signal }) =>
      TemperatureDataSchema.parse(
        await apiJsonFetch("/temperatures", { signal }),
      ),
  }));
  let live = $state<ReturnType<typeof TemperatureDataSchema.parse> | null>(
    null,
  );
  type StreamState = "connecting" | "live" | "reconnecting" | "invalid";
  const streamText: Record<StreamState, () => string> = {
    connecting: m.home_connecting,
    live: m.home_live,
    reconnecting: m.home_reconnecting,
    invalid: m.home_invalid_telemetry,
  };
  let stream = $state<StreamState>("connecting");
  let chart = $state.raw<ChartData>([[], [], [], []]);
  let setpoint = $state<number | undefined>();
  const current = $derived(live ?? initial.data);
  const target = $derived(
    machine.parameters.data?.find((p) => p.name === "brew.setpoint"),
  );
  const shots = $derived(machine.status.data?.shotsSinceBackflush ?? 0);
  const threshold = $derived(
    machine.status.data?.backflushReminderThreshold ?? 50,
  );
  const temperatureChart = $derived([chart[0], chart[1], chart[2]]);
  const heaterChart = $derived([chart[0], chart[3]]);
  $effect(() => {
    if (history.data) chart = historyToChart(history.data);
  });
  onMount(() => {
    const events = new EventSource(`${SERVER_BASE_URL}/events`);
    events.onopen = () => {
      stream = "live";
    };
    events.onerror = () => {
      stream = "reconnecting";
    };
    events.addEventListener("new_temps", (event) => {
      let payload: unknown;
      try {
        payload = JSON.parse(event.data || "null");
      } catch {
        stream = "invalid";
        return;
      }
      const parsed = TemperatureDataSchema.safeParse(payload);
      if (!parsed.success) {
        stream = "invalid";
        return;
      }
      live = parsed.data;
      stream = "live";
      chart = appendSample(chart, parsed.data);
    });
    return () => events.close();
  });
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!target) return;
    try {
      await machine.save.mutateAsync([
        { name: target.name, value: setpoint ?? Number(target.value) },
      ]);
      setpoint = undefined;
      toast.success(m.home_brew_temperature_saved());
    } catch (e) {
      reportError(e);
    }
  }
</script>

<h1 class="sr-only">{m.home_title()}</h1>

<div class="home-grid">
  <Card.Root class="min-w-0"
    ><Card.Header
      ><Card.Title
        role="heading"
        aria-level={2}
        ><Activity size={20} />{m.home_machine_status()}</Card.Title
      ><Card.Action
        ><Badge variant="secondary"
          ><span
            class="status-dot"
            class:offline={stream !== "live"}
          ></span>{streamText[stream]()}</Badge
        ></Card.Action
      ></Card.Header
    ><Card.Content>
      <div class="machine-metrics">
        <div>
          <div class="stat-label">
            <Thermometer
              size={18}
              class="temperature-icon"
            />{m.home_current_temperature()}
          </div>
          <div class="stat-value">
            {current?.currentTemp.toFixed(2) ?? "—"}<span>°C</span>
          </div>
        </div>
        <div>
          <div class="stat-label">
            <Zap
              size={18}
              class="heater-icon"
            />{m.home_current_heater_power()}
          </div>
          <div class="stat-value">
            {current?.heaterPower.toFixed(2) ?? "—"}<span>%</span>
          </div>
        </div>
      </div>
      {#if !current && initial.isError}<p role="alert">
          {m.home_temperature_unavailable()}
        </p>{/if}
      <form
        class="setpoint-form"
        onsubmit={save}
      >
        <div>
          <Label for="brew-temperature">{m.home_setpoint()}</Label>
          <p>
            {m.home_target({ temp: current?.targetTemp.toFixed(1) ?? "—" })}
          </p>
        </div>
        <div class="setpoint-controls">
          <Input
            class=""
            aria-label={m.home_brew_temperature()}
            id="brew-temperature"
            type="number"
            step="any"
            min={target?.min ?? 0}
            max={target?.max ?? 150}
            value={setpoint ?? target?.value ?? ""}
            oninput={(e) => {
              setpoint = e.currentTarget.valueAsNumber;
            }}
            required
            disabled={!target}
          /><Button
            type="submit"
            variant="default"
            disabled={machine.save.isPending || !target}>{m.home_save()}</Button
          >
        </div>
      </form>
    </Card.Content></Card.Root
  >
  <Card.Root class="min-w-0"
    ><Card.Header
      ><Card.Title
        role="heading"
        aria-level={2}
        ><Power size={20} />{m.home_machine_functions()}</Card.Title
      ></Card.Header
    ><Card.Content>
      {#each [{ label: m.home_pid_heating(), route: "/pid", on: machine.status.data?.pidEnabled ?? !!machine.parameters.data?.find((p) => p.name === "pid.enabled")?.value }, { label: m.home_steam_mode(), route: "/steam", on: machine.status.data?.steamMode }] as control}
        <div class="control-row">
          <div>
            <span>{control.label}</span>
            <p>{control.on ? m.home_enabled() : m.home_disabled()}</p>
          </div>
          <SwitchField
            label={control.label}
            checked={!!control.on}
            disabled={machine.command.isPending ||
              machine.status.isError ||
              machine.status.isPending}
            onchange={() => machine.run(control.route)}
          />
        </div>{/each}
      <div class="control-row">
        <span>{m.home_backflush()}</span><Button
          type="button"
          variant="outline"
          disabled={machine.command.isPending ||
            machine.status.isError ||
            machine.status.isPending}
          onclick={() => machine.run("/backflush")}
          >{m.home_toggle_backflush()}</Button
        >
      </div>
      {#if machine.parameters.data?.some((p) => p.name === "hardware.sensors.scale.enabled" && !!p.value)}<div
          class="flex gap-2 mt-4"
        >
          <Button
            type="button"
            variant="outline"
            disabled={machine.command.isPending}
            onclick={() => machine.run("/scale/tare")}
            >{m.home_tare_scale()}</Button
          ><Button
            type="button"
            variant="outline"
            disabled={machine.command.isPending}
            onclick={() => machine.run("/scale/calibration")}
            >{m.home_calibrate_scale()}</Button
          >
        </div>{/if}
    </Card.Content></Card.Root
  >
  <Card.Root class="min-w-0 maintenance-panel"
    ><Card.Header
      ><Card.Title
        role="heading"
        aria-level={2}><Droplets size={20} />{m.home_maintenance()}</Card.Title
      ></Card.Header
    ><Card.Content>
      <div class="maintenance-number">
        {shots}<span> / {threshold}</span>
      </div>
      <p class="text-sm text-muted-foreground mb-3">
        {machine.status.data?.backflushReminderDue
          ? m.home_shots_since_backflush_cleaning()
          : m.home_shots_since_last_backflush()}
      </p>
      <Progress
        max={Math.max(1, threshold)}
        value={shots}
        aria-label={m.home_shots_since_backflush()}
      ></Progress><Button
        type="button"
        variant="outline"
        class="mt-4"
        disabled={machine.command.isPending}
        onclick={() => machine.run("/maintenance/reset-backflush-counter")}
        ><RotateCcw size={15} />{m.home_reset_counter()}</Button
      >
    </Card.Content></Card.Root
  >
  <Card.Root class="min-w-0 chart-panel"
    ><Card.Header
      ><Card.Title
        role="heading"
        aria-level={2}
      >
        <Thermometer
          size={20}
          class="temperature-icon"
        />{m.home_temperature_history()}
      </Card.Title></Card.Header
    ><Card.Content>
      {#if history.isError}<Alert.Root
          role="alert"
          variant="destructive"
          class="mb-4"
          ><Alert.Description>
            {m.home_history_unavailable()}
          </Alert.Description><Alert.Action
            ><Button
              type="button"
              variant="outline"
              size="sm"
              onclick={() => history.refetch()}>{m.home_retry()}</Button
            ></Alert.Action
          ></Alert.Root
        >{/if}<Chart
        data={temperatureChart}
        {dark}
      />
    </Card.Content></Card.Root
  >
  <Card.Root class="min-w-0 chart-panel"
    ><Card.Header
      ><Card.Title
        role="heading"
        aria-level={2}
      >
        <Zap
          size={20}
          class="heater-icon"
        />{m.home_heater_power_history()}
      </Card.Title></Card.Header
    ><Card.Content>
      <Chart
        data={heaterChart}
        kind="heater"
        {dark}
      />
    </Card.Content></Card.Root
  >
</div>
