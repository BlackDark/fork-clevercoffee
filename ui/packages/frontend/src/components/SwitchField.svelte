<script lang="ts">
  import { Switch } from "./ui/switch";
  let {
    id,
    label,
    checked,
    disabled = false,
    onchange,
  }: {
    id?: string;
    label: string;
    checked: boolean;
    disabled?: boolean;
    onchange: (checked: boolean) => void;
  } = $props();
</script>

<Switch
  {id}
  aria-label={label}
  {disabled}
  bind:checked={() => checked, onchange}
/>
