<script lang="ts">
  import { Input } from "@/components/ui/input";
  import SelectField from "./SelectField.svelte";
  import SwitchField from "./SwitchField.svelte";
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
  <SwitchField
    id={parameter.name}
    label={parameter.name}
    checked={!!parameter.value}
    {disabled}
    {onchange}
  />
{:else if isParameterEnum(parameter)}
  <SelectField
    id={parameter.name}
    label={parameter.name}
    value={String(parameter.value)}
    options={(parameter.options ?? []).map((option) => ({
      value: String(option.value),
      label: option.label,
    }))}
    {disabled}
    onchange={(value) => onchange(Number(value))}
  />
{:else if isParameterString(parameter)}
  <Input
    class=""
    id={parameter.name}
    aria-label={parameter.name}
    type={parameter.name.includes("password") ? "password" : "text"}
    value={String(parameter.value)}
    maxlength={parameter.max > 0 ? parameter.max : 64}
    {disabled}
    onchange={(e) => onchange(e.currentTarget.value)}
  />
{:else}
  <Input
    class=""
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
