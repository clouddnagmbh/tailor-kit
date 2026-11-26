<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- Copyright 2026 The RequestTailor Authors -->
<!-- Every primitive in one theme (the gallery shows this once per theme). Dev only. -->
<script lang="ts">
  import { onMount } from "svelte";

  import Avatar from "../Avatar.svelte";
  import Badge from "../Badge.svelte";
  import Banner from "../Banner.svelte";
  import Button from "../Button.svelte";
  import Card from "../Card.svelte";
  import Checkbox from "../Checkbox.svelte";
  import Chip from "../Chip.svelte";
  import Dialog from "../Dialog.svelte";
  import DialogFooter from "../DialogFooter.svelte";
  import Dropdown from "../Dropdown.svelte";
  import EmptyState from "../EmptyState.svelte";
  import Icon from "../Icon.svelte";
  import IconButton from "../IconButton.svelte";
  import { ICON_NAMES } from "../icons";
  import Kbd from "../Kbd.svelte";
  import type { MenuEntry } from "../menu";
  import Progress from "../Progress.svelte";
  import Section from "../Section.svelte";
  import SegmentedControl from "../SegmentedControl.svelte";
  import Select from "../Select.svelte";
  import Skeleton from "../Skeleton.svelte";
  import Spinner from "../Spinner.svelte";
  import SplitButton from "../SplitButton.svelte";
  import Switch from "../Switch.svelte";
  import Tabs from "../Tabs.svelte";
  import TextArea from "../TextArea.svelte";
  import TextField from "../TextField.svelte";
  import Tooltip from "../Tooltip.svelte";

  let { theme, menuOpen = false }: { theme: "light" | "dark"; menuOpen?: boolean } = $props();

  let tab = $state("params");
  let view = $state("split");
  let density = $state("comfortable");
  let url = $state("https://api.example.com/v1/orders/{{orderId}}");
  let search = $state("");
  let message = $state("Add pagination to the orders list\n\nThe API returns `next` links now.");
  let env = $state("staging");
  let checks = $state({ a: true, b: false, c: true });
  let switches = $state({ a: true, b: false });
  let sectionOpen = $state(true);
  let dialogOpen = $state(false);
  let tile = $state("http");
  let menuShown = $state(false);
  // Opened after the first frame, so the menu can measure its anchor.
  onMount(() => {
    if (menuOpen) menuShown = true;
  });
  let pressed = $state(true);

  const menuItems: MenuEntry[] = [
    { heading: "Branch main" },
    { id: "new", label: "New branch…", icon: "branch", shortcut: "Mod+B" },
    { id: "checkout", label: "Switch to…", icon: "sync" },
    {
      id: "merge",
      label: "Merge into current",
      icon: "merge",
      items: [
        { id: "m-feature", label: "feature/pagination" },
        { id: "m-fix", label: "fix/oauth-refresh" },
      ],
    },
    { id: "rename", label: "Rename…", icon: "edit", disabled: true },
    { separator: true },
    { id: "follow", label: "Show remote branches", checked: true },
    { separator: true },
    { id: "delete", label: "Delete branch", icon: "delete", danger: true },
  ];

  const splitItems: MenuEntry[] = [
    { id: "push", label: "Commit and push", icon: "push", shortcut: "Mod+Shift+Enter" },
    { id: "amend", label: "Amend last commit", icon: "commit" },
  ];

  const tiles = [
    { id: "http", title: "HTTP", text: "REST and plain HTTP", icon: "http" },
    { id: "graphql", title: "GraphQL", text: "Queries with schema completion", icon: "graphql" },
    { id: "grpc", title: "gRPC", text: "Unary and streaming calls", icon: "grpc" },
    { id: "websocket", title: "WebSocket", text: "Two-way message streams", icon: "websocket" },
  ];

  const colours = [
    ["surface-1", "--rt-surface-1"],
    ["surface-2", "--rt-surface-2"],
    ["surface-3", "--rt-surface-3"],
    ["sunken", "--rt-surface-sunken"],
    ["panel", "--rt-bg-panel"],
    ["hover", "--rt-bg-hover"],
    ["selected", "--rt-bg-selected"],
    ["border", "--rt-border"],
    ["fg", "--rt-fg"],
    ["fg-muted", "--rt-fg-muted"],
    ["fg-subtle", "--rt-fg-subtle"],
    ["accent", "--rt-accent"],
    ["accent-solid", "--rt-accent-solid"],
    ["accent-subtle", "--rt-accent-subtle"],
    ["ok", "--rt-ok"],
    ["warn", "--rt-warn"],
    ["danger", "--rt-danger"],
  ];
</script>

<div class="specimens">
  <div class="theme-label">
    <Icon name={theme === "dark" ? "moon" : "sun"} size="sm" />
    {theme === "dark" ? "Dark" : "Light"}
  </div>

  <section class="card wide" aria-label="Foundations">
    <h2>Foundations</h2>
    <div class="swatches">
      {#each colours as [name, token] (token)}
        <div class="swatch">
          <span class="chip-colour" style:background="var({token})"></span>
          <span class="swatch-name">{name}</span>
        </div>
      {/each}
    </div>
    <div class="foundation-row">
      <div class="type">
        <div style:font="var(--rt-font-heading)" style:letter-spacing="var(--rt-tracking-tight)">Heading 18 / 24</div>
        <div style:font="var(--rt-font-title)">Title 14 / 20 semibold</div>
        <div style:font="var(--rt-font-body)">Body 13 / 18 — the default for UI text</div>
        <div style:font="var(--rt-font-small)" style:color="var(--rt-fg-muted)">Small 12 / 16 for secondary text</div>
        <div style:font="var(--rt-font-caption)" style:color="var(--rt-fg-subtle)">CAPTION 11 / 14</div>
      </div>
      <div class="scales">
        <div class="spacing">
          {#each [1, 2, 3, 4, 5, 6, 7, 8] as n (n)}
            <span class="space" style:width="var(--rt-space-{n})" title="--rt-space-{n}"></span>
          {/each}
        </div>
        <div class="elevations">
          {#each [1, 2, 3] as n (n)}
            <span class="elev" style:box-shadow="var(--rt-elev-{n})">elev-{n}</span>
          {/each}
        </div>
      </div>
    </div>
  </section>

  <section class="card" aria-label="Buttons">
    <h2>Buttons</h2>
    <div class="row">
      <Button variant="primary" icon="send">Send</Button>
      <Button icon="add">New item</Button>
      <Button variant="ghost" icon="import">Import</Button>
      <Button variant="danger" icon="delete">Delete</Button>
    </div>
    <div class="row">
      <Button variant="primary" size="sm">Save</Button>
      <Button size="sm" iconRight="chevron-down">main</Button>
      <Button variant="ghost" size="sm" icon="history">History</Button>
      <Button variant="primary" loading>Pushing</Button>
      <Button disabled>Disabled</Button>
    </div>
    <div class="row">
      <SplitButton icon="commit" items={splitItems}>Commit 3 files</SplitButton>
      <SplitButton variant="secondary" items={splitItems}>Fetch</SplitButton>
      <span class="divider"></span>
      <IconButton icon="search" label="Search" shortcut="Mod+K" />
      <IconButton icon="sync" label="Sync with remote" />
      <IconButton icon="filter" label="Filter" pressed={pressed} onclick={() => (pressed = !pressed)} />
      <IconButton icon="more" label="More actions" variant="secondary" />
      <IconButton icon="delete" label="Discard changes" variant="danger" />
    </div>
  </section>

  <section class="card" aria-label="Menus">
    <h2>Menu and Dropdown</h2>
    <div class="row">
      <Dropdown items={menuItems} label="Branch actions" icon="branch" bind:open={menuShown} />
      <Dropdown items={menuItems} label="Environment" variant="ghost" size="sm" />
      <Dropdown items={menuItems} menuLabel="Ref">
        {#snippet trigger(props)}
          <Chip {...props} icon="branch" label="feature/pagination" mono caret />
        {/snippet}
      </Dropdown>
    </div>
  </section>

  <section class="card" aria-label="Badges and chips">
    <h2>Badges and chips</h2>
    <div class="row">
      <Badge kind="method" value="GET" />
      <Badge kind="method" value="POST" />
      <Badge kind="method" value="PUT" />
      <Badge kind="method" value="PATCH" />
      <Badge kind="method" value="DELETE" />
      <Badge kind="method" value="GRPC" />
      <span class="divider"></span>
      <Badge kind="status" value="A" />
      <Badge kind="status" value="M" />
      <Badge kind="status" value="D" />
      <Badge kind="status" value="R" />
      <Badge kind="status" value="U" />
      <Badge kind="status" value="?" />
    </div>
    <div class="row">
      <Badge kind="count" value={3} />
      <Badge kind="count" value={128} tone="accent" solid />
      <Badge value="Pro" tone="accent" />
      <Badge value="New" tone="ok" />
      <Badge value="Beta" tone="warn" />
      <Badge value="Breaking" tone="danger" />
      <Badge value="Draft" />
    </div>
    <div class="row">
      <Chip label="All" selected onclick={() => {}} />
      <Chip label="Changed" onclick={() => {}} />
      <Chip label="production" icon="environment" tone="danger" />
      <Chip label="v2.4.0" icon="tag" mono />
      <Chip label="status: 200" onremove={() => {}} />
      <Chip label="staging" icon="environment" caret />
      <Chip label="sm" size="sm" tone="info" />
    </div>
  </section>

  <section class="card" aria-label="Tabs">
    <h2>Tabs and segmented control</h2>
    <Tabs
      label="Sections"
      bind:value={tab}
      items={[
        { id: "params", label: "Params", count: 2 },
        { id: "headers", label: "Headers", count: 5 },
        { id: "body", label: "Body" },
        { id: "auth", label: "Auth", icon: "lock" },
        { id: "scripts", label: "Scripts" },
        { id: "tests", label: "Tests", disabled: true },
      ]}
    />
    <div class="row">
      <SegmentedControl
        label="Diff view"
        bind:value={view}
        options={[
          { value: "split", label: "Side by side" },
          { value: "unified", label: "Unified" },
        ]}
      />
      <SegmentedControl
        label="Layout"
        size="sm"
        bind:value={density}
        options={[
          { value: "comfortable", icon: "file-text", title: "Comfortable" },
          { value: "compact", icon: "code", title: "Compact" },
          { value: "graph", icon: "graph", title: "Graph" },
        ]}
      />
    </div>
  </section>

  <section class="card" aria-label="Fields">
    <h2>Fields</h2>
    <div class="grid2">
      <TextField label="Address" bind:value={url} mono />
      <Select
        label="Environment"
        bind:value={env}
        icon="environment"
        options={[
          { value: "local", label: "Local" },
          { value: "staging", label: "Staging" },
          { value: "production", label: "Production" },
        ]}
      />
      <TextField bind:value={search} placeholder="Search items" icon="search" size="sm">
        {#snippet trailing()}<Kbd keys="Mod+K" size="sm" />{/snippet}
      </TextField>
      <TextField label="Branch name" value="feature/ pagination" error="Branch names cannot contain spaces." />
    </div>
    <TextArea label="Commit message" bind:value={message} rows={3} hint="The first line is the summary." />
  </section>

  <section class="card" aria-label="Choices">
    <h2>Checkbox and switch</h2>
    <div class="grid2">
      <div class="stack">
        <Checkbox bind:checked={checks.a} label="orders/list.yml" />
        <Checkbox bind:checked={checks.b} label="orders/create.yml" description="Modified 2 minutes ago" />
        <Checkbox checked indeterminate label="Select all" />
        <Checkbox disabled label="Locked by policy" />
      </div>
      <div class="stack">
        <Switch bind:checked={switches.a} label="Follow redirects" />
        <Switch bind:checked={switches.b} label="Verify TLS certificates" description="Turn off only for local test servers." />
        <Switch checked size="sm" label="Compact rows" />
      </div>
    </div>
  </section>

  <section class="card" aria-label="Banners">
    <h2>Banners</h2>
    <div class="stack">
      <Banner tone="info" title="This item has scripts">Scripts run only in items you trust.</Banner>
      <Banner tone="ok" dismissible>All 24 tests passed in 1.2 s.</Banner>
      <Banner tone="warn" title="Uncommitted changes" dismissible>
        Switching branches keeps them in a stash.
        {#snippet actions()}<Button size="sm">Stash and switch</Button>{/snippet}
      </Banner>
      <Banner tone="danger" title="Secret found in orders/create.yml">Move the token to the keychain before you commit.</Banner>
    </div>
  </section>

  <section class="card" aria-label="Cards">
    <h2>Cards and sections</h2>
    <div class="tiles">
      {#each tiles as t (t.id)}
        <Card icon={t.icon} title={t.title} description={t.text} padding="sm" selected={tile === t.id} onclick={() => (tile = t.id)} />
      {/each}
    </div>
    <div class="grid2">
      <Section title="Changes" count={3} collapsible bind:open={sectionOpen}>
        {#snippet actions()}
          <IconButton icon="undo" label="Discard all" size="sm" />
          <IconButton icon="add" label="Stage all" size="sm" />
        {/snippet}
        <ul class="rows">
          <li><Badge kind="status" value="M" /> <span class="mono">orders/list.yml</span></li>
          <li><Badge kind="status" value="A" /> <span class="mono">orders/refund.yml</span></li>
          <li><Badge kind="status" value="D" /> <span class="mono">legacy/ping.yml</span></li>
        </ul>
      </Section>
      <Card title="Remote" description="origin · github.com">
        {#snippet actions()}<IconButton icon="settings" label="Remote settings" size="sm" />{/snippet}
        <div class="row tight">
          <Avatar name="Ada Lovelace" />
          <Avatar name="Grace Hopper" />
          <Avatar name="linus@example.com" />
          <Avatar name="Margaret Hamilton" size="lg" />
          <span class="muted">3 ahead · 1 behind</span>
        </div>
        {#snippet footer()}
          <Button size="sm" variant="ghost" icon="pull">Pull</Button>
          <Button size="sm" variant="primary" icon="push">Push</Button>
        {/snippet}
      </Card>
    </div>
  </section>

  <section class="card" aria-label="Feedback">
    <h2>Feedback</h2>
    <div class="row">
      <Spinner size="sm" />
      <Spinner />
      <Spinner size="lg" />
      <span class="divider"></span>
      <Tooltip text="Copy as cURL" shortcut="Mod+Shift+C"><Button size="sm" icon="copy">Hover me</Button></Tooltip>
      <div class="rt-tooltip static">Send item <span class="rt-tooltip-keys"><kbd>⌘</kbd><kbd>↵</kbd></span></div>
      <span class="divider"></span>
      <Kbd keys="Mod+Enter" />
      <Kbd keys="Mod+Shift+P" />
      <Kbd keys="Escape" />
    </div>
    <div class="stack">
      <Progress label="Upload" value={60} />
      <Progress label="Run" value={100} tone="ok" size="sm" />
      <Progress label="Quota" value={85} tone="warn" />
      <Progress label="Failed" value={30} tone="danger" size="sm" />
      <Progress label="Loading" />
    </div>
    <div class="grid2">
      <div class="stack">
        <div class="row tight">
          <Skeleton circle height="28px" />
          <div class="grow"><Skeleton lines={2} /></div>
        </div>
        <Skeleton height="64px" />
      </div>
      <div class="empty">
        <EmptyState icon="history" title="No history yet" text="Items you send appear here, newest first." size="sm">
          {#snippet actions()}<Button size="sm" variant="primary" icon="send">Send an item</Button>{/snippet}
        </EmptyState>
      </div>
    </div>
  </section>

  <section class="card" aria-label="Dialog">
    <h2>Dialog</h2>
    <p class="muted">Header, body, sticky footer; Escape closes, focus stays inside.</p>
    <div class="row">
      <Button variant="primary" icon="branch" onclick={() => (dialogOpen = true)}>Open dialog</Button>
    </div>
  </section>

  <section class="card wide" aria-label="Icons">
    <h2>Icons <span class="muted">({ICON_NAMES.length}, Lucide)</span></h2>
    <div class="icons">
      {#each ICON_NAMES as name (name)}
        <div class="icon-cell" title={name}>
          <Icon {name} />
          <span>{name}</span>
        </div>
      {/each}
    </div>
  </section>
</div>

{#if dialogOpen}
  <Dialog title="Create branch" description="From main at 4f2c1ab" icon="branch" onclose={() => (dialogOpen = false)}>
    <div class="stack">
      <TextField label="Name" value="feature/pagination" mono autofocus />
      <Checkbox checked label="Switch to the new branch" />
      <Banner tone="info">Your 2 uncommitted changes move with you.</Banner>
    </div>
    <DialogFooter>
      {#snippet start()}<Button variant="ghost" size="sm" iconRight="external-link">What is a branch?</Button>{/snippet}
      <Button onclick={() => (dialogOpen = false)}>Cancel</Button>
      <Button variant="primary" onclick={() => (dialogOpen = false)}>Create branch</Button>
    </DialogFooter>
  </Dialog>
{/if}

<style>
  .specimens {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 520px), 1fr));
    gap: var(--rt-space-6);
    align-items: stretch;
  }
  .theme-label {
    grid-column: 1 / -1;
    display: inline-flex;
    align-items: center;
    gap: var(--rt-space-2);
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    letter-spacing: var(--rt-tracking-caps);
    text-transform: uppercase;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
    min-width: 0;
    padding: var(--rt-space-6) var(--rt-space-7) var(--rt-space-7);
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-lg);
    background: var(--rt-surface-1);
    box-shadow: var(--rt-elev-1);
  }
  .wide {
    grid-column: 1 / -1;
  }
  h2 {
    margin: 0;
    font: var(--rt-font-title);
    letter-spacing: var(--rt-tracking-tight);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--rt-space-4);
  }
  .tight {
    flex-wrap: nowrap;
    gap: var(--rt-space-3);
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-5);
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--rt-space-6);
    align-items: start;
  }
  .grow {
    flex: 1;
  }
  .divider {
    width: 1px;
    height: 20px;
    background: var(--rt-border);
  }
  .muted {
    margin: 0;
    color: var(--rt-fg-muted);
    font-weight: var(--rt-weight-regular);
  }
  .mono {
    font-family: var(--rt-font-mono);
    font-size: var(--rt-text-small);
  }
  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
    gap: var(--rt-space-4);
  }
  .swatch {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-2);
  }
  .chip-colour {
    height: 36px;
    border-radius: var(--rt-radius-md);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--rt-fg) 10%, transparent);
  }
  .swatch-name {
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    font-family: var(--rt-font-mono);
    font-weight: var(--rt-weight-regular);
  }
  .foundation-row {
    display: grid;
    grid-template-columns: 1.2fr 1fr;
    gap: var(--rt-space-7);
    align-items: center;
  }
  .type {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-3);
  }
  .scales {
    display: flex;
    flex-direction: column;
    gap: var(--rt-space-6);
  }
  .spacing {
    display: flex;
    align-items: flex-end;
    gap: var(--rt-space-4);
  }
  .space {
    height: 24px;
    border-radius: 2px;
    background: color-mix(in srgb, var(--rt-accent) 55%, transparent);
  }
  .elevations {
    display: flex;
    gap: var(--rt-space-6);
  }
  .elev {
    display: grid;
    place-items: center;
    width: 96px;
    height: 56px;
    border: 1px solid var(--rt-border);
    border-radius: var(--rt-radius-lg);
    background: var(--rt-surface-3);
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: var(--rt-space-4);
  }
  .rows {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .rows li {
    display: flex;
    align-items: center;
    gap: var(--rt-space-4);
    height: var(--rt-control-md);
    padding: 0 var(--rt-space-3);
    border-radius: var(--rt-radius-md);
  }
  .rows li:hover {
    background: var(--rt-bg-hover);
  }
  .rt-tooltip.static {
    position: relative;
    animation: none;
  }
  .empty {
    border: 1px dashed var(--rt-border-strong);
    border-radius: var(--rt-radius-lg);
    background: var(--rt-bg-panel);
  }
  .icons {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: var(--rt-space-2);
  }
  .icon-cell {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--rt-space-3);
    padding: var(--rt-space-5) var(--rt-space-2) var(--rt-space-4);
    border-radius: var(--rt-radius-md);
    color: var(--rt-fg);
  }
  .icon-cell:hover {
    background: var(--rt-bg-hover);
  }
  .icon-cell span {
    color: var(--rt-fg-muted);
    font: var(--rt-font-caption);
    font-weight: var(--rt-weight-regular);
  }
</style>
