<script lang="ts">
  import { Button } from "@/components/ui/button";
  import * as Card from "@/components/ui/card";
  import { Coffee, ExternalLink } from "@lucide/svelte";
  const version = APP_VERSION;
</script>

<div class="page-heading">
  <div>
    <h1>About CleverCoffee</h1>
    <p>CleverCoffee firmware and BrewUI.</p>
  </div>
</div>
<Card.Root class="panel"
  ><Card.Content>
    <Coffee
      size={40}
      class="text-primary mb-6"
    />
    <Card.Title
      role="heading"
      aria-level={2}
      class="text-2xl mb-3">Better control. Better coffee.</Card.Title
    >
    <p class="text-muted-foreground leading-relaxed">
      CleverCoffee brings precise temperature control and thoughtful machine
      management to your espresso ritual. BrewUI puts live telemetry,
      configuration, maintenance, and updates in one place.
    </p>
    <div class="change-row my-6">
      <span>UI version</span><strong>{version}</strong>
    </div>
    <div class="flex flex-wrap gap-3">
      <Button
        variant="default"
        href="https://clevercoffee.de/"
        target="_blank"
        rel="noreferrer">CleverCoffee <ExternalLink size={15} /></Button
      ><Button
        variant="outline"
        href="https://github.com/BlackDark/fork-clevercoffee"
        target="_blank"
        rel="noreferrer">This project</Button
      ><Button
        variant="outline"
        href="https://github.com/rancilio-pid/clevercoffee"
        target="_blank"
        rel="noreferrer">Upstream</Button
      ><Button
        variant="outline"
        href="https://discord.gg/Kq5RFznuU4"
        target="_blank"
        rel="noreferrer">Community</Button
      >
    </div>
  </Card.Content></Card.Root
>
