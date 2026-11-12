<script lang="ts">
  import * as m from "@/paraglide/messages.js";
  import * as Tabs from "@/components/ui/tabs";
  import * as Alert from "@/components/ui/alert";
  import { Skeleton } from "@/components/ui/skeleton";
  import { Button } from "@/components/ui/button";
  import { Input } from "@/components/ui/input";
  import { Label } from "@/components/ui/label";
  import { Badge } from "@/components/ui/badge";
  import * as Card from "@/components/ui/card";
  import ParameterHint from "../components/ParameterHint.svelte";
  import { toast } from "svelte-sonner";
  import { reportError } from "@/lib/notify";
  import {
    Search,
    Save,
    RotateCcw,
    AlertTriangle,
    Settings2,
  } from "@lucide/svelte";
  import { useMachine } from "@/lib/machine.svelte";
  import { groups2, parameterGroups } from "@/lib/parameter-groups";
  import { getParameterHelp, getParameterLabel } from "@/lib/parameter-text";
  import {
    areRequiredParametersMet,
    getMissingRequiredParametersMessage,
  } from "@/lib/parameter-utils";
  import { parameterRequiresReboot } from "@/lib/parameter-reboot-required";
  import type { Parameter } from "@/lib/parameter-types";
  import ParameterInput from "../components/ParameterInput.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  let { filter = "behavior" }: { filter?: string } = $props();
  const machine = useMachine();
  const draft = machine.draft;
  let search = $state("");
  let confirm = $state(false);
  let reboot = $state(false);
  const parameters = $derived(
    (machine.parameters.data ?? []).map((p) => ({
      ...p,
      value: draft.edits[p.name] ?? p.value,
    })),
  );
  const changes = $derived(
    parameters.filter(
      (p) =>
        Object.hasOwn(draft.edits, p.name) &&
        p.value !==
          machine.parameters.data?.find((s) => s.name === p.name)?.value,
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
  function categoryLabel(category: string) {
    if (category === "behavior") return m.config_category_behavior();
    if (category === "system") return m.config_category_system();
    return m.config_category_hardware();
  }
  async function save() {
    const submitted = changes.map((p) => ({ name: p.name, value: p.value }));
    try {
      await machine.save.mutateAsync(submitted);
      reboot = submitted.some((p) => parameterRequiresReboot(p.name));
      draft.edits = {};
      confirm = false;
      toast.success(m.config_parameters_saved());
    } catch (e) {
      reportError(e);
      confirm = false;
    }
  }
</script>

<div class="page-heading">
  <div>
    <h1>{m.config_title()}</h1>
    <p>{m.config_subtitle()}</p>
  </div>
</div>
<div class="settings-toolbar">
  <nav aria-label={m.config_parameter_categories()}>
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
          <Tabs.Trigger value={category}>{categoryLabel(category)}</Tabs.Trigger
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
      aria-label={m.config_search_aria()}
      placeholder={m.config_search_placeholder()}
      bind:value={search}
    />
  </div>
</div>
{#if filter === "hardware"}<Alert.Root
    variant="warning"
    class="mb-4"
    ><AlertTriangle size={18} /><Alert.Description class="text-inherit">
      <span>{m.config_hardware_warning()}</span>
    </Alert.Description></Alert.Root
  >{/if}

{#if reboot}<Alert.Root class="mb-4"
    ><Alert.Description>
      {m.config_reboot_needed()}
    </Alert.Description><Alert.Action class="flex gap-2"
      ><Button
        href="#/system"
        variant="outline"
        size="sm">{m.config_restart_in_system()}</Button
      ><Button
        type="button"
        variant="ghost"
        size="sm"
        onclick={() => (reboot = false)}>{m.config_dismiss()}</Button
      ></Alert.Action
    ></Alert.Root
  >{/if}
{#if machine.parameters.isPending}<div
    role="status"
    aria-label={m.config_loading_parameters()}
    class="grid gap-5"
  >
    <Skeleton class="h-48 rounded-xl" /><Skeleton class="h-48 rounded-xl" />
  </div>{:else if machine.parameters.isError}<Alert.Root
    role="alert"
    variant="destructive"
    class="mb-4"
    ><Alert.Description>
      {machine.parameters.error.message}
    </Alert.Description><Alert.Action
      ><Button
        type="button"
        variant="outline"
        size="sm"
        onclick={() => machine.parameters.refetch()}>{m.config_retry()}</Button
      ></Alert.Action
    ></Alert.Root
  >{:else if !groups2[filter]}<p>{m.config_unknown_category()}</p>{:else}
  <form
    onsubmit={(e) => {
      e.preventDefault();
      confirm = true;
    }}
  >
    {#each [...sections, ...(filter === "system" && unmapped.length ? [{ key: "other", label: m.config_other_device_parameters(), items: unmapped }] : [])] as group}
      <Card.Root class="min-w-0 parameter-panel"
        ><Card.Header
          ><Card.Title
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
          </Card.Title><Card.Action
            ><Badge variant="secondary"
              >{group.items.length === 1
                ? m.config_setting({ count: group.items.length })
                : m.config_settings({ count: group.items.length })}</Badge
            ></Card.Action
          ></Card.Header
        ><Card.Content>
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
                        role="img"
                        aria-label={m.config_edited()}
                      ></span>{/if}</Label
                  >
                  {#if getParameterHelp(parameter.name)}<ParameterHint
                      id={`help-${parameter.name}`}
                      text={(getParameterHelp(parameter.name) ?? "").replace(
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
                      label={getParameterLabel(parameter.name) ||
                        parameter.name}
                      disabled={machine.save.isPending}
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
          {m.config_no_matching_parameters()}
        </Alert.Description></Alert.Root
      >{/if}
    <div class="save-bar">
      <span
        >{changes.length
          ? changes.length === 1
            ? m.config_unsaved_change({ count: changes.length })
            : m.config_unsaved_changes({ count: changes.length })
          : m.config_all_changes_saved()}</span
      >
      <div class="flex gap-2">
        <Button
          type="button"
          variant="outline"
          onclick={() => {
            draft.edits = {};
          }}
          disabled={!changes.length || machine.save.isPending}
          ><RotateCcw size={15} />{m.config_discard()}</Button
        ><Button
          type="submit"
          variant="default"
          disabled={!changes.length || machine.save.isPending}
          ><Save size={15} />{m.config_review_and_save()}</Button
        >
      </div>
    </div>
  </form>
{/if}
<ConfirmDialog
  bind:open={confirm}
  title={m.config_save_confirm_title()}
  description={m.config_save_confirm_description()}
  confirm={save}
  busy={machine.save.isPending}
  label={m.config_save_changes()}
>
  {#each changes as parameter}<div class="change-row">
      <strong>{getParameterLabel(parameter.name) || parameter.name}</strong
      ><span
        >{displayValue(
          parameter,
          machine.parameters.data?.find((p) => p.name === parameter.name)
            ?.value,
        )} → {displayValue(parameter, parameter.value)}</span
      >
    </div>{/each}
</ConfirmDialog>
