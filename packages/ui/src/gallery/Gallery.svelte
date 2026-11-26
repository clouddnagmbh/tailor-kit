<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- The primitive gallery (`?gallery=1` in the web dev server; never in production builds).
     `&theme=light|dark|both` picks the theme, `&menu=1` opens the example menu. -->
<script lang="ts">
  import SegmentedControl from "../SegmentedControl.svelte";
  import Specimens from "./Specimens.svelte";

  type Mode = "light" | "dark" | "both";
  const params = new URLSearchParams(location.search);
  const initial = params.get("theme");
  let mode = $state<Mode>(initial === "light" || initial === "dark" ? initial : "both");
  const menuOpen = params.get("menu") === "1";

  $effect(() => {
    document.documentElement.setAttribute("data-theme", mode === "dark" ? "dark" : "light");
  });
</script>

<div class="gallery" class:both={mode === "both"}>
  <header>
    <div class="brand">
      <span class="logo" aria-hidden="true"></span>
      <div>
        <h1>Tailor Kit UI</h1>
        <p>Primitives and tokens v2 · packages/ui/src</p>
      </div>
    </div>
    <SegmentedControl
      label="Theme"
      bind:value={mode}
      options={[
        { value: "light", label: "Light", icon: "sun" },
        { value: "dark", label: "Dark", icon: "moon" },
        { value: "both", label: "Both", icon: "compare" },
      ]}
    />
  </header>
  <main>
    {#if mode === "both"}
      <div class="column" data-theme="light"><Specimens theme="light" /></div>
      <div class="column" data-theme="dark"><Specimens theme="dark" /></div>
    {:else}
      {#key mode}
        <div class="column"><Specimens theme={mode} {menuOpen} /></div>
      {/key}
    {/if}
  </main>
</div>

<style>
  .gallery {
    min-height: 100vh;
    background: var(--rt-bg-panel);
    color: var(--rt-fg);
    font: var(--rt-font-body);
  }
  header {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--rt-space-6);
    padding: var(--rt-space-5) var(--rt-space-8);
    border-bottom: 1px solid var(--rt-border);
    background: color-mix(in srgb, var(--rt-bg) 82%, transparent);
    backdrop-filter: blur(12px);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: var(--rt-space-5);
  }
  .logo {
    width: 28px;
    height: 28px;
    border-radius: var(--rt-radius-md);
    background: linear-gradient(135deg, var(--rt-accent-solid), var(--rt-lane-4));
    box-shadow:
      var(--rt-elev-1),
      inset 0 1px 0 rgb(255 255 255 / 0.25);
  }
  h1 {
    margin: 0;
    font: var(--rt-font-title);
    letter-spacing: var(--rt-tracking-tight);
  }
  p {
    margin: 0;
    color: var(--rt-fg-muted);
    font: var(--rt-font-small);
  }
  main {
    display: grid;
    gap: 0;
  }
  .both main {
    grid-template-columns: 1fr 1fr;
  }
  .column {
    min-width: 0;
    padding: var(--rt-space-7) var(--rt-space-8) var(--rt-space-8);
    background: var(--rt-bg-panel);
    color: var(--rt-fg);
  }
  .both .column {
    padding: var(--rt-space-7) var(--rt-space-7) var(--rt-space-8);
  }
</style>
