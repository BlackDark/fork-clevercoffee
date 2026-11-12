<script lang="ts">
  import * as m from "@/paraglide/messages.js";
  import * as Select from "./ui/select";
  let {
    id,
    label,
    value,
    options,
    disabled = false,
    onchange,
  }: {
    id: string;
    label: string;
    value: string;
    options: { value: string; label: string }[];
    disabled?: boolean;
    onchange: (value: string) => void;
  } = $props();
</script>

<Select.Root
  type="single"
  items={options}
  {disabled}
  bind:value={() => value, onchange}
>
  <Select.Trigger
    {id}
    aria-label={label}
    class="w-full"
  >
    <Select.Value placeholder={m.select_an_option()} />
  </Select.Trigger>
  <Select.Content
    sideOffset={4}
    collisionPadding={12}
  >
    {#each options as option (option.value)}
      <Select.Item
        value={option.value}
        label={option.label}
      >
        {option.label}
      </Select.Item>
    {/each}
  </Select.Content>
</Select.Root>
