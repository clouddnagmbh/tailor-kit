// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// One list-navigation keymap for the kit's lists (graph, changes, file tree, file history), in the
// style of Vim's normal mode. A list describes itself with a `VimList`; the keymap does the rest.
// It is off unless the host asks for it (`GitHost.keyboard.vim`): `vimKeys` is given a function that
// returns the list while Vim keys are on and `null` otherwise, and never acts on `null`. It never
// acts while focus is in a text field.
//
//   j k            next, previous          gg G        first, last
//   Ctrl+d Ctrl+u  half a page down, up    h l         collapse / go to parent, expand / go in
//   /              find (the list's own filter if it has one, else a small inline find)
//   n N            next, previous match    Enter       activate     Esc   leave the find

/** What a list offers the keymap. Everything but the first three is optional. */
export interface VimList {
  count(): number;
  /** The selected item, or -1. */
  index(): number;
  /** Selects item `i` (already clamped) and keeps it in view. */
  select(i: number): void;
  /** Items per page; half of it is the step of Ctrl+d and Ctrl+u. Default 20. */
  page?(): number;
  /** `h` (`false`) and `l` (`true`) on the selected item: collapse or expand a folder. */
  open?(expand: boolean): void;
  /** `Enter` on the selected item. Without it Enter is left to the list. */
  activate?(): void;
  /** `/`: the list shows its own filter; `true` when it did. Without it the inline find is used. */
  find?(): boolean;
  /** `n` (1) and `N` (-1) for the list's own filter; `true` when there was a match to go to. */
  repeat?(dir: 1 | -1): boolean;
  /** The text of every item, for the inline find. */
  labels?(): string[];
}

/** The part of a `KeyboardEvent` the keymap reads. */
export interface VimKeyEvent {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  target: unknown;
}

/** Whether `target` is a text field (or inside one): the keys never act there. */
export function inTextField(target: unknown): boolean {
  const el = target as { closest?: (s: string) => unknown; isContentEditable?: boolean } | null;
  if (!el || typeof el.closest !== "function") return false;
  return Boolean(el.isContentEditable) || el.closest("input, textarea, select, [contenteditable='true'], [contenteditable='']") !== null;
}

export interface VimKeymap {
  /** Handles one key press; `true`: it was a Vim key (the caller stops the event). */
  handle(e: VimKeyEvent): boolean;
  /** The text of the inline find while it is being typed (`null`: not open). */
  readonly finding: string | null;
}

/** Index of the next label that contains `query` (case-insensitive), searching from `from` in
 *  direction `dir` and wrapping; -1 when none does. */
export function nextMatch(labels: string[], query: string, from: number, dir: 1 | -1): number {
  const q = query.toLowerCase();
  if (!q || labels.length === 0) return -1;
  for (let k = 1; k <= labels.length; k++) {
    const i = (((from + dir * k) % labels.length) + labels.length) % labels.length;
    if (labels[i]!.toLowerCase().includes(q)) return i;
  }
  return -1;
}

export function createVimKeymap(getList: () => VimList | null, onFinding: (text: string | null) => void = () => {}): VimKeymap {
  let pendingG = false;
  let finding: string | null = null;
  let query = "";

  const setFinding = (text: string | null) => {
    finding = text;
    onFinding(text);
  };

  function go(list: VimList, i: number) {
    const n = list.count();
    if (n === 0) return;
    list.select(Math.max(0, Math.min(n - 1, i)));
  }

  function jump(list: VimList, dir: 1 | -1) {
    if (list.repeat?.(dir)) return;
    const labels = list.labels?.();
    if (!labels || !query) return;
    const i = nextMatch(labels, query, list.index(), dir);
    if (i >= 0) list.select(i);
  }

  function handle(e: VimKeyEvent): boolean {
    const list = getList();
    if (!list) {
      pendingG = false;
      if (finding !== null) setFinding(null);
      return false;
    }
    if (e.metaKey || e.altKey || inTextField(e.target)) return false;

    if (finding !== null) {
      // Typing a find: every key is part of it until Enter or Esc.
      if (e.ctrlKey) return false;
      if (e.key === "Escape") {
        setFinding(null);
        return true;
      }
      if (e.key === "Enter") {
        query = finding;
        setFinding(null);
        return true;
      }
      if (e.key === "Backspace") {
        setFinding(finding.slice(0, -1));
        return true;
      }
      if (e.key.length === 1) {
        setFinding(finding + e.key);
        const labels = list.labels?.() ?? [];
        const i = nextMatch(labels, finding, list.index() - 1, 1);
        if (i >= 0) list.select(i);
        return true;
      }
      return false;
    }

    const half = Math.max(1, Math.floor((list.page?.() ?? 20) / 2));
    const i = list.index();
    if (e.ctrlKey) {
      pendingG = false;
      if (e.key === "d") go(list, i + half);
      else if (e.key === "u") go(list, i - half);
      else return false;
      return true;
    }
    const wasG = pendingG;
    pendingG = false;
    switch (e.key) {
      case "j":
        go(list, i + 1);
        return true;
      case "k":
        go(list, i < 0 ? 0 : i - 1);
        return true;
      case "g":
        if (wasG) go(list, 0);
        else pendingG = true;
        return true;
      case "G":
        go(list, list.count() - 1);
        return true;
      case "h":
      case "l":
        if (!list.open) return false;
        list.open(e.key === "l");
        return true;
      case "/":
        if (list.find?.()) return true;
        if (!list.labels) return false;
        setFinding("");
        return true;
      case "n":
        jump(list, 1);
        return true;
      case "N":
        jump(list, -1);
        return true;
      case "Enter":
        if (!list.activate) return false;
        list.activate();
        return true;
      case "Escape":
        return false;
      default:
        return false;
    }
  }

  return {
    handle,
    get finding() {
      return finding;
    },
  };
}

/** Svelte action: `use:vimKeys={() => (host.keyboard?.vim ? list : null)}`. Listens on `node` (its
 *  focused descendants' key presses bubble to it), stops the events it handled so the list's own
 *  handler does not move a second time, and shows the inline find as a small status line. */
export function vimKeys(node: HTMLElement, getList: () => VimList | null) {
  let bar: HTMLElement | null = null;
  const show = (text: string | null) => {
    if (text === null) {
      bar?.remove();
      bar = null;
      return;
    }
    if (!bar) {
      bar = node.ownerDocument.createElement("div");
      bar.setAttribute("role", "status");
      bar.setAttribute("data-testid", "vim-find");
      bar.style.cssText =
        "position:sticky;bottom:0;left:0;padding:2px 8px;font:var(--rt-font-small);background:var(--rt-bg-raised, #fff);color:var(--rt-fg, inherit);border-top:1px solid var(--rt-border, #ccc);";
      node.appendChild(bar);
    }
    bar.textContent = `/${text}`;
  };
  let current = getList;
  const keymap = createVimKeymap(() => current(), show);
  const onkeydown = (e: KeyboardEvent) => {
    if (e.defaultPrevented) return;
    if (keymap.handle(e)) {
      e.preventDefault();
      e.stopPropagation();
    }
  };
  node.addEventListener("keydown", onkeydown);
  return {
    update(next: () => VimList | null) {
      current = next;
    },
    destroy() {
      node.removeEventListener("keydown", onkeydown);
      bar?.remove();
    },
  };
}
