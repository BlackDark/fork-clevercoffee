<script lang="ts">
  import { cn } from "../../../utils";

  export let variant = "default";
  export let className = "";

  const alertClasses = {
    base: "relative w-full rounded-lg border p-4 [&>svg~*]:pl-7 [&>svg+div]:translate-y-[-3px] [&>svg]:absolute [&>svg]:left-4 [&>svg]:top-4 [&>svg]:text-foreground",
    variants: {
      default: "bg-background text-foreground",
      destructive: "border-destructive/50 text-destructive dark:border-destructive [&>svg]:text-destructive",
    }
  };

  const getAlertClasses = () => {
    return cn(
      alertClasses.base,
      alertClasses.variants[variant] || alertClasses.variants.default,
      className
    );
  };
</script>

<div
  class={getAlertClasses()}
  role="alert"
  {...$$restProps}
>
  <slot />
</div>
