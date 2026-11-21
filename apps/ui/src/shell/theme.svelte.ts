// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Light, dark or the OS preference. Sets `data-theme` on <html> (tokens.css reads it);
// "system" removes the attribute. Remembered per device.

import { load, store } from "./storage";

export type ThemePreference = "system" | "light" | "dark";

const KEY = "theme";
const isPreference = (v: unknown): v is ThemePreference => v === "system" || v === "light" || v === "dark";
const NEXT: Record<ThemePreference, ThemePreference> = { system: "light", light: "dark", dark: "system" };

export class Theme {
  preference = $state<ThemePreference>(load(KEY, "system", isPreference));

  constructor(private readonly root: HTMLElement | undefined = globalThis.document?.documentElement) {
    this.apply();
  }

  set(preference: ThemePreference): void {
    this.preference = preference;
    store(KEY, preference);
    this.apply();
  }

  /** system → light → dark → system (the header button). */
  cycle(): void {
    this.set(NEXT[this.preference]);
  }

  /** Flips what is shown now between light and dark (the palette command). */
  toggle(): void {
    this.set(this.effective() === "dark" ? "light" : "dark");
  }

  effective(): "light" | "dark" {
    if (this.preference !== "system") return this.preference;
    return globalThis.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }

  private apply(): void {
    if (!this.root) return;
    if (this.preference === "system") this.root.removeAttribute("data-theme");
    else this.root.setAttribute("data-theme", this.preference);
  }
}
