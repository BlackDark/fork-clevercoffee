<script lang="ts">
  import { Search, Save, RotateCcw, AlertTriangle } from "@lucide/svelte";
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
  let message = $state("");
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
      message = "Parameters saved successfully.";
    } catch (e) {
      message = e instanceof Error ? e.message : String(e);
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
  <nav
    class="tabs"
    aria-label="Parameter categories"
  >
    {#each ["behavior", "system", "hardware"] as category}<a
        href={`#/config/${category}`}
        class:active={filter === category}
        aria-current={filter === category ? "page" : undefined}
        >{category[0].toUpperCase() + category.slice(1)}</a
      >{/each}
  </nav>
  <div class="search-box">
    <Search size={16} /><input
      aria-label="Search parameters"
      placeholder="Find a parameter…"
      bind:value={search}
    />
  </div>
</div>
{#if filter === "hardware"}<div class="notice warning">
    <AlertTriangle size={18} /><span
      >Incorrect hardware settings can activate the pump, heater, or valve
      unexpectedly. Check relay, sensor, and switch settings before saving and
      restarting.</span
    >
  </div>{/if}
{#if message}<div
    class="notice"
    role="status"
  >
    {message}
  </div>{/if}
{#if reboot}<div class="notice">
    Some saved settings need a reboot to take effect. <a
      href="#/system"
      class="text-link">Restart in System</a
    ><button
      class="text-link"
      onclick={() => (reboot = false)}>Dismiss</button
    >
  </div>{/if}
{#if m.parameters.isPending}<p role="status">
    Loading parameters…
  </p>{:else if m.parameters.isError}<div
    role="alert"
    class="notice error"
  >
    {m.parameters.error.message}<button
      class="text-link"
      onclick={() => m.parameters.refetch()}>Retry</button
    >
  </div>{:else if !groups2[filter]}<p>Unknown parameter category.</p>{:else}
  <form
    onsubmit={(e) => {
      e.preventDefault();
      confirm = true;
    }}
  >
    {#each [...sections, ...(filter === "system" && unmapped.length ? [{ key: "other", label: "Other device parameters", items: unmapped }] : [])] as group}
      <section class="panel parameter-panel">
        <div class="panel-heading">
          <h2>{group.label}</h2>
          <span class="badge">{group.items.length} settings</span>
        </div>
        {#each group.items as parameter (parameter.name)}{@const enabled =
            areRequiredParametersMet(parameter, parameters)}
          <div class="parameter-row">
            <div>
              <label
                for={parameter.name}
                class="font-medium"
                >{getParameterLabel(parameter.name) ||
                  parameter.name}{#if Object.hasOwn(draft.edits, parameter.name)}<span
                    class="edited-dot"
                    aria-label="Edited"
                  ></span>{/if}</label
              >
              <p class="parameter-key">{parameter.name}</p>
              {#if parameterHelpTexts[parameter.name]}<p class="parameter-help">
                  {parameterHelpTexts[parameter.name].replace(/<[^>]*>/g, "")}
                </p>{/if}{#if !enabled}<p class="text-sm text-amber-700">
                  {getMissingRequiredParametersMessage(parameter, parameters)}
                </p>{/if}
            </div>
            <div class="parameter-control">
              <ParameterInput
                {parameter}
                disabled={!enabled || m.save.isPending}
                onchange={(value) => {
                  draft.edits = { ...draft.edits, [parameter.name]: value };
                }}
              />
            </div>
          </div>
        {/each}
      </section>{/each}
    {#if !sections.length && !(filter === "system" && unmapped.length)}<p
        class="notice"
      >
        No matching parameters.
      </p>{/if}
    <div class="save-bar">
      <span
        >{changes.length
          ? `${changes.length} unsaved change${changes.length === 1 ? "" : "s"}`
          : "All changes saved"}</span
      >
      <div class="flex gap-2">
        <button
          type="button"
          class="btn secondary"
          onclick={() => {
            draft.edits = {};
          }}
          disabled={!changes.length || m.save.isPending}
          ><RotateCcw size={15} />Discard</button
        ><button
          class="btn"
          disabled={!changes.length || m.save.isPending}
          ><Save size={15} />Review & save</button
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
