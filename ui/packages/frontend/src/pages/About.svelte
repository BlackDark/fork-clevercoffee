<script lang="ts">
  import { Button } from "@/components/ui/button";
  import * as Card from "@/components/ui/card";
  import { Coffee, ExternalLink } from "@lucide/svelte";
  import * as m from "@/paraglide/messages.js";
  const version = APP_VERSION;
</script>

<div class="page-heading">
  <div>
    <h1>{m.about_title()}</h1>
    <p>{m.about_subtitle()}</p>
  </div>
</div>
<Card.Root class="min-w-0"
  ><Card.Content>
    <Coffee
      size={40}
      class="text-primary mb-6"
    />
    <Card.Title
      role="heading"
      aria-level={2}
      class="text-2xl mb-3">{m.about_tagline()}</Card.Title
    >
    <p class="text-muted-foreground leading-relaxed">
      {m.about_body()}
    </p>
    <div class="change-row my-6">
      <span>{m.about_ui_version()}</span><strong>{version}</strong>
    </div>
    <div class="flex flex-wrap gap-3">
      <Button
        variant="default"
        href="https://clevercoffee.de/"
        target="_blank"
        rel="noreferrer">CleverCoffee <ExternalLink size={15} /></Button
      ><Button
        variant="outline"
        href="https://github.com/BlackDark/clevercoffee-rs"
        target="_blank"
        rel="noreferrer">{m.about_this_project()}</Button
      ><Button
        variant="outline"
        href="https://github.com/rancilio-pid/clevercoffee"
        target="_blank"
        rel="noreferrer">{m.about_upstream()}</Button
      ><Button
        variant="outline"
        href="https://discord.gg/Kq5RFznuU4"
        target="_blank"
        rel="noreferrer">{m.about_community()}</Button
      >
    </div>
  </Card.Content></Card.Root
>
