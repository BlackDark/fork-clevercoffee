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
  let chart = $state.raw<ChartData>([[], [], [], []]);
  let setpoint = $state<number | undefined>();
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
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!target) return;
    try {
      await m.save.mutateAsync([
        { name: target.name, value: setpoint ?? Number(target.value) },
      ]);
      setpoint = undefined;
      toast.success("Brew temperature saved");
    } catch (e) {
      reportError(e);
    }
  }
</script>

<h1 class="sr-only">Home</h1>

<div class="home-grid">
  <Card.Root class="panel"
    ><Card.Content>
      <div class="panel-heading">
        <Card.Title
          role="heading"
          aria-level={2}><Activity size={20} />Machine Status</Card.Title
        >
        <Badge variant="secondary"
          ><span
            class="status-dot"
            class:offline={stream !== "Live"}
          ></span>{stream}</Badge
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
          <Label for="brew-temperature">Setpoint (°C)</Label>
          <p>Target: {current?.targetTemp.toFixed(1) ?? "—"} °C</p>
        </div>
        <div class="setpoint-controls">
          <Input
            class=""
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
          /><Button
            type="submit"
            variant="default"
            disabled={m.save.isPending || !target}>Save</Button
          >
        </div>
      </form>
    </Card.Content></Card.Root
  >
  <Card.Root class="panel"
    ><Card.Content>
      <div class="panel-heading">
        <Card.Title
          role="heading"
          aria-level={2}><Power size={20} />Machine Functions</Card.Title
        >
      </div>
      {#each [{ label: "PID heating", route: "/pid", on: m.status.data?.pidEnabled ?? !!m.parameters.data?.find((p) => p.name === "pid.enabled")?.value }, { label: "Steam mode", route: "/steam", on: m.status.data?.steamMode }] as control}
        <div class="control-row">
          <div>
            <span>{control.label}</span>
            <p>{control.on ? "Enabled" : "Disabled"}</p>
          </div>
          <SwitchField
            label={control.label}
            checked={!!control.on}
            disabled={m.command.isPending ||
              m.status.isError ||
              m.status.isPending}
            onchange={() => m.run(control.route)}
          />
        </div>{/each}
      <div class="control-row">
        <span>Backflush</span><Button
          type="button"
          variant="outline"
          disabled={m.command.isPending ||
            m.status.isError ||
            m.status.isPending}
          onclick={() => m.run("/backflush")}>Toggle backflush</Button
        >
      </div>
      {#if m.parameters.data?.some((p) => p.name === "hardware.sensors.scale.enabled" && !!p.value)}<div
          class="flex gap-2 mt-4"
        >
          <Button
            type="button"
            variant="outline"
            disabled={m.command.isPending}
            onclick={() => m.run("/scale/tare")}>Tare scale</Button
          ><Button
            type="button"
            variant="outline"
            disabled={m.command.isPending}
            onclick={() => m.run("/scale/calibration")}>Calibrate scale</Button
          >
        </div>{/if}
    </Card.Content></Card.Root
  >
  <Card.Root class="panel maintenance-panel"
    ><Card.Content>
      <div class="panel-heading">
        <Card.Title
          role="heading"
          aria-level={2}><Droplets size={20} />Maintenance</Card.Title
        >
      </div>
      <div class="maintenance-number">
        {shots}<span> / {threshold}</span>
      </div>
      <p class="text-sm text-muted-foreground mb-3">
        Shots since last backflush{m.status.data?.backflushReminderDue
          ? " · Cleaning recommended"
          : ""}
      </p>
      <Progress
        max={Math.max(1, threshold)}
        value={shots}
        aria-label="Shots since backflush"
      ></Progress><Button
        type="button"
        variant="outline"
        class="mt-4"
        disabled={m.command.isPending}
        onclick={() => m.run("/maintenance/reset-backflush-counter")}
        ><RotateCcw size={15} />Reset counter</Button
      >
    </Card.Content></Card.Root
  >
  <Card.Root class="panel chart-panel"
    ><Card.Content>
      <div class="panel-heading">
        <Card.Title
          role="heading"
          aria-level={2}
        >
          <Thermometer
            size={20}
            class="temperature-icon"
          />Temperature History
        </Card.Title>
      </div>
      {#if history.isError}<Alert.Root
          role="alert"
          variant="destructive"
          class="mb-4"
          ><Alert.Description>
            History unavailable.
          </Alert.Description><Alert.Action
            ><Button
              type="button"
              variant="outline"
              size="sm"
              onclick={() => history.refetch()}>Retry</Button
            ></Alert.Action
          ></Alert.Root
        >{/if}<Chart
        data={temperatureChart}
        {dark}
      />
    </Card.Content></Card.Root
  >
  <Card.Root class="panel chart-panel"
    ><Card.Content>
      <div class="panel-heading">
        <Card.Title
          role="heading"
          aria-level={2}
        >
          <Zap
            size={20}
            class="heater-icon"
          />Heater Power History
        </Card.Title>
      </div>
      <Chart
        data={heaterChart}
        kind="heater"
        {dark}
      />
    </Card.Content></Card.Root
  >
</div>
