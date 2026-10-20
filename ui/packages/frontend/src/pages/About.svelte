<script lang="ts">
  import { Coffee, ExternalLink } from "@lucide/svelte";
  const version = APP_VERSION;
</script>

<div class="page-heading">
  <div>
    <div class="eyebrow">BUILT FOR YOUR DAILY CUP</div>
    <h1>About CleverCoffee</h1>
    <p>Precision, a little curiosity, and a love of good coffee.</p>
  </div>
</div>
<section class="panel max-w-3xl">
  <Coffee
    size={40}
    class="text-primary mb-6"
  />
  <h2 class="text-2xl mb-3">Better control. Better coffee.</h2>
  <p class="text-muted-foreground leading-relaxed">
    CleverCoffee brings precise temperature control and thoughtful machine
    management to your espresso ritual. BrewUI puts live telemetry,
    configuration, maintenance, and updates in one place.
  </p>
  <div class="change-row my-6">
    <span>UI version</span><strong>{version}</strong>
  </div>
  <div class="flex flex-wrap gap-3">
    <a
      class="btn"
      href="https://clevercoffee.de/"
      target="_blank"
      rel="noreferrer">CleverCoffee <ExternalLink size={15} /></a
    ><a
      class="btn secondary"
      href="https://github.com/BlackDark/fork-clevercoffee"
      target="_blank"
      rel="noreferrer">This project</a
    ><a
      class="btn secondary"
      href="https://github.com/rancilio-pid/clevercoffee"
      target="_blank"
      rel="noreferrer">Upstream</a
    ><a
      class="btn secondary"
      href="https://discord.gg/Kq5RFznuU4"
      target="_blank"
      rel="noreferrer">Community</a
    >
  </div>
</section>
