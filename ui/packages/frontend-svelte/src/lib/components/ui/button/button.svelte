<script lang="ts">
  import { cn } from "../../../utils";

  export let variant = "default";
  export let size = "default";
  export let href = undefined;
  export let type = "button";
  export let disabled = false;
  export let className = "";

  const buttonVariants = {
    base: "inline-flex items-center justify-center rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:opacity-50 disabled:pointer-events-none ring-offset-background",
    variant: {
      default: "bg-primary text-primary-foreground hover:bg-primary/90",
      destructive: "bg-destructive text-destructive-foreground hover:bg-destructive/90",
      outline: "border border-input hover:bg-accent hover:text-accent-foreground",
      secondary: "bg-secondary text-secondary-foreground hover:bg-secondary/80",
      ghost: "hover:bg-accent hover:text-accent-foreground",
      link: "underline-offset-4 hover:underline text-primary"
    },
    size: {
      default: "h-10 py-2 px-4",
      sm: "h-9 px-3 rounded-md",
      lg: "h-11 px-8 rounded-md",
      icon: "h-10 w-10"
    }
  };

  const getVariantClasses = () => {
    return cn(
      buttonVariants.base,
      buttonVariants.variant[variant] || buttonVariants.variant.default,
      buttonVariants.size[size] || buttonVariants.size.default,
      className
    );
  };
</script>

{#if href}
  <a
    {href}
    class={getVariantClasses()}
    aria-disabled={disabled}
    tabindex={disabled ? -1 : undefined}
    {...$$restProps}
  >
    <slot />
  </a>
{:else}
  <button
    {type}
    {disabled}
    class={getVariantClasses()}
    {...$$restProps}
  >
    <slot />
  </button>
{/if}
