<script lang="ts">
  import {
    isParameterBoolean,
    isParameterEnum,
    isParameterString,
    ParameterTypes,
    type Parameter,
  } from "@/lib/parameter-types";
  let {
    parameter,
    disabled = false,
    onchange,
  }: {
    parameter: Parameter;
    disabled?: boolean;
    onchange: (value: string | number | boolean) => void;
  } = $props();
</script>

{#if isParameterBoolean(parameter)}
  <input
    class="toggle"
    aria-label={parameter.name}
    id={parameter.name}
    type="checkbox"
    role="switch"
    checked={!!parameter.value}
    {disabled}
    onchange={(e) => onchange(e.currentTarget.checked)}
  />
{:else if isParameterEnum(parameter)}
  <select
    class="input"
    id={parameter.name}
    aria-label={parameter.name}
    value={String(parameter.value)}
    {disabled}
    onchange={(e) => onchange(Number(e.currentTarget.value))}
    >{#each parameter.options ?? [] as option}<option value={option.value}
        >{option.label}</option
      >{/each}</select
  >
{:else if isParameterString(parameter)}
  <input
    class="input"
    id={parameter.name}
    aria-label={parameter.name}
    type={parameter.name.includes("password") ? "password" : "text"}
    value={String(parameter.value)}
    maxlength={parameter.max > 0 ? parameter.max : 64}
    {disabled}
    onchange={(e) => onchange(e.currentTarget.value)}
  />
{:else}
  <input
    class="input"
    id={parameter.name}
    aria-label={parameter.name}
    type="number"
    value={Number(parameter.value)}
    min={parameter.min}
    max={parameter.max}
    step={parameter.type === ParameterTypes.FLOAT ||
    parameter.type === ParameterTypes.DOUBLE
      ? "any"
      : 1}
    {disabled}
    onchange={(e) => onchange(e.currentTarget.valueAsNumber)}
  />
{/if}
