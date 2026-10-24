<script lang="ts">
  import { Menu, X, Sun, Moon } from "@lucide/svelte";
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
  let menuOpen = $state(false);
  let menuButton: HTMLButtonElement;
  let dark = $state(false);
  let theme = $state("system");
  const links = [
    { href: "/", label: "Home" },
    { href: "/config/behavior", label: "Configuration" },
    { href: "/system", label: "System" },
    { href: "/about", label: "About" },
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
      menuOpen = false;
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

<svelte:window
  onbeforeunload={guard}
  onkeydown={(event) => {
    if (event.key === "Escape" && menuOpen) {
      menuOpen = false;
      menuButton?.focus();
    }
  }}
  onclick={(event) => {
    if (
      menuOpen &&
      !event
        .composedPath()
        .some(
          (node) =>
            node instanceof Element && node.classList.contains("topbar"),
        )
    )
      menuOpen = false;
  }}
  onresize={() => {
    if (innerWidth >= 768) menuOpen = false;
  }}
/>

<a
  href="#main"
  onclick={(event) => {
    event.preventDefault();
    document.getElementById("main")?.focus();
  }}
  class="skip-link">Skip to content</a
>
<div class="app-layout">
  <header class="topbar">
    <a
      href="#/"
      class="brand"
      aria-label="CleverCoffee"
      ><img
        src={`${import.meta.env.BASE_URL}logo.png`}
        alt=""
        width="24"
        height="24"
      /></a
    >
    <nav
      id="primary-navigation"
      aria-label="Main navigation"
      class="header-nav"
      class:open={menuOpen}
    >
      {#each links as link}
        {@const active =
          link.href === "/"
            ? route === "/"
            : route.startsWith(link.href.split("/").slice(0, 2).join("/"))}
        <a
          href={`#${link.href}`}
          class:active
          aria-current={active ? "page" : undefined}
          onclick={() => {
            menuOpen = false;
          }}>{link.label}</a
        >
      {/each}
    </nav>
    <div class="header-actions">
      <span
        class="connection-status"
        aria-label="Device connection"
        ><span
          class="status-dot"
          class:offline={machine.status.isError}
        ></span>{machine.status.isPending
          ? "Connecting…"
          : machine.status.isError
            ? "Offline"
            : "Online"}</span
      >
      <button
        class="icon-btn"
        onclick={toggleTheme}
        aria-label={dark ? "Switch to light theme" : "Switch to dark theme"}
        >{#if dark}<Sun size={18} />{:else}<Moon size={18} />{/if}</button
      >
      <button
        class="icon-btn menu-button"
        bind:this={menuButton}
        aria-label={menuOpen ? "Close navigation menu" : "Open navigation menu"}
        aria-expanded={menuOpen}
        aria-controls="primary-navigation"
        onclick={() => {
          menuOpen = !menuOpen;
        }}
        >{#if menuOpen}<X size={20} />{:else}<Menu size={20} />{/if}</button
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
        Backflush recommended · {machine.status.data.shotsSinceBackflush} shots since
        cleaning.
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
    <footer>CleverCoffee</footer>
  </main>
</div>
