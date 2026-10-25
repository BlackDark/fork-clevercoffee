<script lang="ts">
  import * as Tabs from "@/components/ui/tabs";
  import * as Alert from "@/components/ui/alert";
  import { Button } from "@/components/ui/button";
  import { Input } from "@/components/ui/input";
  import { Label } from "@/components/ui/label";
  import { Badge } from "@/components/ui/badge";
  import * as Card from "@/components/ui/card";
  import ParameterHint from "../components/ParameterHint.svelte";
  import { toast } from "svelte-sonner";
  import {
    Search,
    Save,
    RotateCcw,
    AlertTriangle,
    Settings2,
  } from "@lucide/svelte";
  import { useMachine } from "@/lib/machine.svelte";
  import { groups2, parameterGroups } from "@/lib/parameter-groups";
  import { getParameterLabel } from "@/lib/parameter-labels";
  import { parameterHelpTexts } from "@/lib/parameter-help-texts";
  import {
    areRequiredParametersMet,
    getMissingRequiredParametersMessage,
  } from "@/lib/parameter-utils";
  import { parameterRequiresReboot } from "@/lib/parameter-reboot-required";
  import type { Parameter } from "@/lib/parameter-types";
  import ParameterInput from "../components/ParameterInput.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  let { filter = "behavior" }: { filter?: string } = $props();
  const m = useMachine();
  const draft = m.draft;
  let search = $state("");
  let confirm = $state(false);
  let reboot = $state(false);
  const parameters = $derived(
    (m.parameters.data ?? []).map((p) => ({
      ...p,
      value: draft.edits[p.name] ?? p.value,
    })),
  );
  const changes = $derived(
    parameters.filter(
      (p) =>
        Object.hasOwn(draft.edits, p.name) &&
        p.value !== m.parameters.data?.find((s) => s.name === p.name)?.value,
    ),
  );
  const sections = $derived(
    parameterGroups
      .filter((g) => groups2[filter]?.includes(g.key))
      .map((g) => ({
        ...g,
        items: parameters.filter(
          (p) =>
            g.parameters.includes(p.name) &&
            `${getParameterLabel(p.name)} ${p.name}`
              .toLowerCase()
              .includes(search.toLowerCase()),
        ),
      }))
      .filter((g) => g.items.length),
  );
  const unmapped = $derived(
    parameters.filter(
      (p) =>
        !parameterGroups.some((g) => g.parameters.includes(p.name)) &&
        `${getParameterLabel(p.name)} ${p.name}`
          .toLowerCase()
          .includes(search.toLowerCase()),
    ),
  );

  function displayValue(p: Parameter, value: unknown) {
    return p.name.includes("password") ? "••••••" : String(value);
  }
  async function save() {
    const submitted = changes.map((p) => ({ name: p.name, value: p.value }));
    try {
      await m.save.mutateAsync(submitted);
      reboot = submitted.some((p) => parameterRequiresReboot(p.name));
      draft.edits = {};
      confirm = false;
      toast.success("Parameters saved successfully.");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
      confirm = false;
    }
  }
</script>

<div class="page-heading">
  <div>
    <h1>Configuration</h1>
    <p>Configure machine behavior, system settings, and hardware.</p>
  </div>
</div>
<div class="settings-toolbar">
  <nav aria-label="Parameter categories">
    <Tabs.Root
      value={filter}
      onValueChange={(value) => {
        if (value && value !== filter) {
          filter = value;
          location.hash = `/config/${value}`;
        }
      }}
    >
      <Tabs.List>
        {#each ["behavior", "system", "hardware"] as category}
          <Tabs.Trigger value={category}
            >{category[0].toUpperCase() + category.slice(1)}</Tabs.Trigger
          >
        {/each}
      </Tabs.List>
    </Tabs.Root>
  </nav>
  <div class="relative w-full sm:w-64">
    <Search
      size={16}
      class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
    /><Input
      class="pl-9"
      aria-label="Search parameters"
      placeholder="Find a parameter…"
      bind:value={search}
    />
  </div>
</div>
{#if filter === "hardware"}<Alert.Root
    class="mb-4 border-amber-300 bg-amber-50 text-amber-950 dark:border-amber-800 dark:bg-amber-950/40 dark:text-amber-100"
    ><AlertTriangle size={18} /><Alert.Description class="text-inherit">
      <span
        >Incorrect hardware settings can activate the pump, heater, or valve
        unexpectedly. Check relay, sensor, and switch settings before saving and
        restarting.</span
      >
    </Alert.Description></Alert.Root
  >{/if}

{#if reboot}<Alert.Root class="mb-4"
    ><Alert.Description>
      Some saved settings need a reboot to take effect. <Button
        href="#/system"
        variant="link"
        class="h-auto p-0">Restart in System</Button
      ><Button
        type="button"
        variant="link"
        class="h-auto p-0"
        onclick={() => (reboot = false)}>Dismiss</Button
      >
    </Alert.Description></Alert.Root
  >{/if}
{#if m.parameters.isPending}<p role="status">
    Loading parameters…
  </p>{:else if m.parameters.isError}<Alert.Root
    role="alert"
    variant="destructive"
    class="mb-4"
    ><Alert.Description>
      {m.parameters.error.message}<Button
        type="button"
        variant="link"
        class="h-auto p-0"
        onclick={() => m.parameters.refetch()}>Retry</Button
      >
    </Alert.Description></Alert.Root
  >{:else if !groups2[filter]}<p>Unknown parameter category.</p>{:else}
  <form
    onsubmit={(e) => {
      e.preventDefault();
      confirm = true;
    }}
  >
    {#each [...sections, ...(filter === "system" && unmapped.length ? [{ key: "other", label: "Other device parameters", items: unmapped }] : [])] as group}
      <Card.Root class="panel parameter-panel"
        ><Card.Content>
          <div class="panel-heading">
            <Card.Title
              role="heading"
              aria-level={2}
            >
              <span
                class="section-icon"
                class:purple={/pid|mqtt/i.test(group.key)}
                class:amber={/power|relay|switch/i.test(group.key)}
                class:teal={/brew|water|maintenance|scale/i.test(group.key)}
                ><Settings2 size={19} /></span
              >{group.label}
            </Card.Title>
            <Badge variant="secondary">{group.items.length} settings</Badge>
          </div>
          <div class="parameter-grid">
            {#each group.items as parameter (parameter.name)}{@const enabled =
                areRequiredParametersMet(parameter, parameters)}
              <div class="parameter-row">
                <div class="parameter-label">
                  <Label
                    for={parameter.name}
                    class="font-medium"
                    >{getParameterLabel(parameter.name) ||
                      parameter.name}{#if Object.hasOwn(draft.edits, parameter.name)}<span
                        class="edited-dot"
                        aria-label="Edited"
                      ></span>{/if}</Label
                  >
                  {#if parameterHelpTexts[parameter.name]}<ParameterHint
                      id={`help-${parameter.name}`}
                      text={parameterHelpTexts[parameter.name].replace(
                        /<[^>]*>/g,
                        "",
                      )}
                    />{/if}
                </div>
                <div class="parameter-control">
                  {#if !enabled}<ParameterHint
                      id={`disabled-${parameter.name}`}
                      unavailable
                      text={getMissingRequiredParametersMessage(
                        parameter,
                        parameters,
                      )}
                    />{:else}<ParameterInput
                      {parameter}
                      disabled={m.save.isPending}
                      onchange={(value) => {
                        draft.edits = {
                          ...draft.edits,
                          [parameter.name]: value,
                        };
                      }}
                    />{/if}
                </div>
              </div>
            {/each}
          </div>
        </Card.Content></Card.Root
      >{/each}
    {#if !sections.length && !(filter === "system" && unmapped.length)}<Alert.Root
        class="mb-4"
        ><Alert.Description>
          No matching parameters.
        </Alert.Description></Alert.Root
      >{/if}
    <div class="save-bar">
      <span
        >{changes.length
          ? `${changes.length} unsaved change${changes.length === 1 ? "" : "s"}`
          : "All changes saved"}</span
      >
      <div class="flex gap-2">
        <Button
          type="button"
          variant="outline"
          onclick={() => {
            draft.edits = {};
          }}
          disabled={!changes.length || m.save.isPending}
          ><RotateCcw size={15} />Discard</Button
        ><Button
          type="submit"
          variant="default"
          disabled={!changes.length || m.save.isPending}
          ><Save size={15} />Review & save</Button
        >
      </div>
    </div>
  </form>
{/if}
<ConfirmDialog
  bind:open={confirm}
  title="Save parameter changes?"
  description="Only changed settings will be sent to the device."
  confirm={save}
  busy={m.save.isPending}
  label="Save changes"
>
  {#each changes as parameter}<div class="change-row">
      <strong>{getParameterLabel(parameter.name) || parameter.name}</strong
      ><span
        >{displayValue(
          parameter,
          m.parameters.data?.find((p) => p.name === parameter.name)?.value,
        )} → {displayValue(parameter, parameter.value)}</span
      >
    </div>{/each}
</ConfirmDialog>
