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
    label = parameter.name,
    disabled = false,
    onchange,
  }: {
    parameter: Parameter;
    label?: string;
    disabled?: boolean;
    onchange: (value: string | number | boolean) => void;
  } = $props();
</script>

{#if isParameterBoolean(parameter)}
  <SwitchField
    id={parameter.name}
    {label}
    checked={!!parameter.value}
    {disabled}
    {onchange}
  />
{:else if isParameterEnum(parameter)}
  <SelectField
    id={parameter.name}
    {label}
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
    aria-label={label}
    type={parameter.name.includes("password") ? "password" : "text"}
    value={String(parameter.value)}
    maxlength={parameter.max != null && parameter.max > 0 ? parameter.max : 64}
    {disabled}
    onchange={(e) => onchange(e.currentTarget.value)}
  />
{:else}
  <Input
    class=""
    id={parameter.name}
    aria-label={label}
    type="number"
    value={Number(parameter.value)}
    min={parameter.min}
    max={parameter.max}
    step={parameter.type === ParameterTypes.FLOAT ||
    parameter.type === ParameterTypes.DOUBLE
      ? "any"
      : 1}
    {disabled}
    onchange={(e) => {
      const value = e.currentTarget.valueAsNumber;
      if (Number.isFinite(value)) onchange(value);
    }}
  />
{/if}
