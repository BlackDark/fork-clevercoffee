<script lang="ts">
  import {
    Coffee,
    LayoutDashboard,
    Settings2,
    Server,
    Info,
    Sun,
    Moon,
    ExternalLink,
  } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { createMachine } from "@/lib/machine.svelte";
  import Home from "../pages/Home.svelte";
  import Config from "../pages/Config.svelte";
  import System from "../pages/System.svelte";
  import About from "../pages/About.svelte";
  const machine = createMachine();
  let route = $state(
    location.hash.slice(1) ||
      location.pathname.replace(/^.*\/ui\/?/, "/") ||
      "/",
  );
  let dark = $state(false);
  let theme = $state("system");
  const links = [
    { href: "/", label: "Dashboard", icon: LayoutDashboard },
    { href: "/config/behavior", label: "Parameters", icon: Settings2 },
    { href: "/system", label: "System", icon: Server },
    { href: "/about", label: "About", icon: Info },
  ];
  onMount(() => {
    const media = matchMedia("(prefers-color-scheme: dark)");
    try {
      theme = localStorage.getItem("theme") || "system";
    } catch {
      theme = "system";
    }
    const sync = () => {
      dark = theme === "dark" || (theme === "system" && media.matches);
    };
    sync();
    const hash = () => {
      route = location.hash.slice(1) || "/";
    };
    window.addEventListener("hashchange", hash);
    media.addEventListener("change", sync);
    return () => {
      window.removeEventListener("hashchange", hash);
      media.removeEventListener("change", sync);
    };
  });
  $effect(() => {
    document.documentElement.classList.toggle("dark", dark);
  });
  function toggleTheme() {
    dark = !dark;
    theme = dark ? "dark" : "light";
    try {
      localStorage.setItem("theme", theme);
    } catch {
      /* Storage may be disabled. */
    }
  }
  async function wake() {
    try {
      await machine.command.mutateAsync("/wake");
    } catch {
      /* Error is displayed below. */
    }
  }
  function guard(event: BeforeUnloadEvent) {
    if (Object.keys(machine.draft.edits).length) {
      event.preventDefault();
      event.returnValue = "";
    }
  }
</script>

<svelte:window onbeforeunload={guard} />

<a
  href="#main"
  onclick={(event) => {
    event.preventDefault();
    document.getElementById("main")?.focus();
  }}
  class="skip-link">Skip to content</a
>
<div class="app-layout">
  <aside class="sidebar">
    <a
      href="#/"
      class="brand"
      ><span class="brand-icon"><Coffee size={24} /></span><span
        >CleverCoffee<small>BREWUI</small></span
      ></a
    >
    <div class="nav-caption">YOUR MACHINE</div>
    <nav aria-label="Main navigation">
      {#each links as link}<a
          href={`#${link.href}`}
          class:active={link.href === "/"
            ? route === "/"
            : route.startsWith(link.href.split("/").slice(0, 2).join("/"))}
          aria-current={(
            link.href === "/"
              ? route === "/"
              : route.startsWith(link.href.split("/").slice(0, 2).join("/"))
          )
            ? "page"
            : undefined}><link.icon size={19} />{link.label}</a
        >{/each}
    </nav>
    <div class="sidebar-bottom">
      <span class="flex items-center gap-2"
        ><span
          class="status-dot"
          class:offline={machine.status.isError}
        ></span>{machine.status.isPending
          ? "Connecting…"
          : machine.status.isError
            ? "Device unavailable"
            : "Device connected"}</span
      ><small>Made for a better daily ritual.</small>
    </div>
  </aside>
  <div class="main-column">
    <header class="topbar">
      <span class="text-sm text-muted-foreground">Your coffee, precisely.</span>
      <div class="flex items-center gap-4">
        <a
          href="https://github.com/BlackDark/fork-clevercoffee"
          target="_blank"
          rel="noreferrer"
          class="github-link">GitHub <ExternalLink size={13} /></a
        ><button
          class="icon-btn"
          onclick={toggleTheme}
          aria-label={dark ? "Switch to light theme" : "Switch to dark theme"}
          >{#if dark}<Sun size={18} />{:else}<Moon size={18} />{/if}</button
        >
      </div>
    </header>
    <main
      id="main"
      tabindex="-1"
    >
      {#if machine.status.isError}<div
          class="notice error"
          role="alert"
        >
          Cannot reach the device. Controls may be out of date. <button
            class="text-link"
            onclick={() => machine.status.refetch()}>Retry connection</button
          >
        </div>{/if}
      {#if machine.status.data?.isStandby}<div class="notice">
          The machine is in standby. <button
            class="text-link"
            onclick={wake}
            disabled={machine.command.isPending}>Wake up</button
          >
        </div>{/if}
      {#if machine.status.data?.backflushReminderDue && route !== "/"}<div
          class="notice"
        >
          Backflush recommended · {machine.status.data.shotsSinceBackflush} shots
          since cleaning.
          <a
            class="text-link"
            href="#/">View maintenance</a
          >
        </div>{/if}
      {#if machine.command.isError}<div
          class="notice error"
          role="alert"
        >
          {machine.command.error.message}
        </div>{/if}
      {#if route === "/"}<Home
          {dark}
        />{:else if route.startsWith("/config/")}<Config
          filter={route.split("/")[2]}
        />{:else if route === "/system"}<System
        />{:else if route === "/about"}<About />{:else}<h1>Page not found</h1>
        <a
          href="#/"
          class="btn">Back to dashboard</a
        >{/if}
      <footer>
        CleverCoffee <span>Thoughtful control. Better coffee.</span>
      </footer>
    </main>
  </div>
</div>
