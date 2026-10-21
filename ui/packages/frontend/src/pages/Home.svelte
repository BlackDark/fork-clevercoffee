<script lang="ts">
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
  let { dark = false }: { dark?: boolean } = $props();
  const m = useMachine();
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
  let stream = $state("Connecting");
  let chart = $state<ChartData>([[], [], [], []]);
  let setpoint = $state<number | undefined>();
  let message = $state("");
  const current = $derived(live ?? initial.data);
  const target = $derived(
    m.parameters.data?.find((p) => p.name === "brew.setpoint"),
  );
  const shots = $derived(m.status.data?.shotsSinceBackflush ?? 0);
  const threshold = $derived(m.status.data?.backflushReminderThreshold ?? 50);
  const temperatureChart = $derived([chart[0], chart[1], chart[2]]);
  const heaterChart = $derived([chart[0], chart[3]]);
  $effect(() => {
    if (history.data) chart = historyToChart(history.data);
  });
  onMount(() => {
    const events = new EventSource(`${SERVER_BASE_URL}/events`);
    events.onopen = () => {
      stream = "Live";
    };
    events.onerror = () => {
      stream = "Reconnecting";
    };
    events.addEventListener("new_temps", (event) => {
      let payload: unknown;
      try {
        payload = JSON.parse(event.data || "null");
      } catch {
        stream = "Invalid telemetry";
        return;
      }
      const parsed = TemperatureDataSchema.safeParse(payload);
      if (!parsed.success) {
        stream = "Invalid telemetry";
        return;
      }
      live = parsed.data;
      stream = "Live";
      chart = appendSample(chart, parsed.data);
    });
    return () => events.close();
  });
  async function command(route: string) {
    message = "";
    try {
      await m.command.mutateAsync(route);
      message = "Device updated";
    } catch (e) {
      message = e instanceof Error ? e.message : String(e);
    }
  }
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!target) return;
    message = "";
    try {
      await m.save.mutateAsync([
        { name: target.name, value: setpoint ?? Number(target.value) },
      ]);
      setpoint = undefined;
      message = "Brew temperature saved";
    } catch (e) {
      message = String(e);
    }
  }
</script>

<h1 class="sr-only">Home</h1>
{#if message}<p
    class="notice"
    role="status"
  >
    {message}
  </p>{/if}
<div class="home-grid">
  <section class="panel">
    <div class="panel-heading">
      <h2><Activity size={20} />Machine Status</h2>
      <span class="badge"
        ><span
          class="status-dot"
          class:offline={stream !== "Live"}
        ></span>{stream}</span
      >
    </div>
    <div class="machine-metrics">
      <div>
        <div class="stat-label">
          <Thermometer
            size={18}
            class="temperature-icon"
          />Current Temperature
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
          />Current Heater Power
        </div>
        <div class="stat-value">
          {current?.heaterPower.toFixed(2) ?? "—"}<span>%</span>
        </div>
      </div>
    </div>
    {#if !current && initial.isError}<p role="alert">
        Temperature unavailable.
      </p>{/if}
    <form
      class="setpoint-form"
      onsubmit={save}
    >
      <div>
        <label for="brew-temperature">Setpoint (°C)</label>
        <p>Target: {current?.targetTemp.toFixed(1) ?? "—"} °C</p>
      </div>
      <div class="setpoint-controls">
        <input
          class="input"
          aria-label="Brew temperature"
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
        /><button
          class="btn"
          disabled={m.save.isPending || !target}>Save</button
        >
      </div>
    </form>
  </section>
  <section class="panel">
    <div class="panel-heading">
      <h2><Power size={20} />Machine Functions</h2>
    </div>
    {#each [{ label: "PID heating", route: "/pid", on: m.status.data?.pidEnabled ?? !!m.parameters.data?.find((p) => p.name === "pid.enabled")?.value }, { label: "Steam mode", route: "/steam", on: m.status.data?.steamMode }] as control}
      <div class="control-row">
        <div>
          <span>{control.label}</span>
          <p>{control.on ? "Enabled" : "Disabled"}</p>
        </div>
        <button
          class="pill-toggle"
          role="switch"
          aria-label={control.label}
          aria-checked={!!control.on}
          disabled={m.command.isPending ||
            m.status.isError ||
            m.status.isPending}
          onclick={() => command(control.route)}><span></span></button
        >
      </div>{/each}
    <div class="control-row">
      <span>Backflush</span><button
        class="btn secondary"
        disabled={m.command.isPending || m.status.isError || m.status.isPending}
        onclick={() => command("/backflush")}>Toggle backflush</button
      >
    </div>
    {#if m.parameters.data?.some((p) => p.name === "hardware.sensors.scale.enabled" && !!p.value)}<div
        class="flex gap-2 mt-4"
      >
        <button
          class="btn secondary"
          disabled={m.command.isPending}
          onclick={() => command("/scale/tare")}>Tare scale</button
        ><button
          class="btn secondary"
          disabled={m.command.isPending}
          onclick={() => command("/scale/calibration")}>Calibrate scale</button
        >
      </div>{/if}
  </section>
  <section class="panel chart-panel">
    <div class="panel-heading">
      <h2>
        <Thermometer
          size={20}
          class="temperature-icon"
        />Temperature History
      </h2>
    </div>
    {#if history.isError}<div
        class="notice error"
        role="alert"
      >
        History unavailable. <button
          class="text-link"
          onclick={() => history.refetch()}>Retry</button
        >
      </div>{/if}<Chart
      data={temperatureChart}
      {dark}
    />
  </section>
  <section class="panel chart-panel">
    <div class="panel-heading">
      <h2>
        <Zap
          size={20}
          class="heater-icon"
        />Heater Power History
      </h2>
    </div>
    <Chart
      data={heaterChart}
      kind="heater"
      {dark}
    />
  </section>
  <section class="panel maintenance-panel">
    <div class="panel-heading"><h2><Droplets size={20} />Maintenance</h2></div>
    <div class="maintenance-number">
      {shots}<span> / {threshold} shots</span>
    </div>
    <p class="text-sm text-muted-foreground mb-3">
      Shots since last backflush{m.status.data?.backflushReminderDue
        ? " · Cleaning recommended"
        : ""}
    </p>
    <progress
      max={Math.max(1, threshold)}
      value={shots}
      aria-label="Shots since backflush"
    ></progress><button
      class="btn secondary mt-4"
      disabled={m.command.isPending}
      onclick={() => command("/maintenance/reset-backflush-counter")}
      ><RotateCcw size={15} />Reset counter</button
    >
  </section>
</div>
