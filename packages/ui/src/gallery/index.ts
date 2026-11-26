// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// The primitive gallery, a development page (`?gallery=1` in the web dev server). Shells load
// it only behind `import.meta.env.DEV`, so production builds leave it out.

import { mount } from "svelte";

import "../styles/tokens.css";
import "../styles/base.css";
import Gallery from "./Gallery.svelte";

export function startGallery(target: HTMLElement) {
  return mount(Gallery, { target });
}
