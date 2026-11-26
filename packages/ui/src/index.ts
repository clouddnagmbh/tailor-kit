// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// UI primitives, icons and helpers. New UI builds on these and the tokens v2 in
// `@tailor-kit/ui/styles/tokens.css`; no ad-hoc buttons or colours. Guide: ./README.md,
// gallery: `@tailor-kit/ui/gallery` (`?gallery=1` in a web dev server).

export { default as Avatar } from "./Avatar.svelte";
export { default as Badge } from "./Badge.svelte";
export { default as Banner } from "./Banner.svelte";
export { default as Button } from "./Button.svelte";
export { default as Card } from "./Card.svelte";
export { default as Checkbox } from "./Checkbox.svelte";
export { default as Chip } from "./Chip.svelte";
export { default as Dialog } from "./Dialog.svelte";
export { default as DialogFooter } from "./DialogFooter.svelte";
export { default as Dropdown, type TriggerProps } from "./Dropdown.svelte";
export { default as EmptyState } from "./EmptyState.svelte";
export { default as Icon } from "./Icon.svelte";
export { default as IconButton } from "./IconButton.svelte";
export { default as Kbd } from "./Kbd.svelte";
export { default as Menu } from "./Menu.svelte";
export { default as Progress } from "./Progress.svelte";
export { default as Section } from "./Section.svelte";
export { default as SegmentedControl, type SegmentOption } from "./SegmentedControl.svelte";
export { default as Select, type SelectOption } from "./Select.svelte";
export { default as Skeleton } from "./Skeleton.svelte";
export { default as Spinner } from "./Spinner.svelte";
export { default as SplitButton } from "./SplitButton.svelte";
export { default as Switch } from "./Switch.svelte";
export { default as Tabs, tabId, tabPanelId, type TabItem } from "./Tabs.svelte";
export { default as TextArea } from "./TextArea.svelte";
export { default as TextField } from "./TextField.svelte";
export { default as Tooltip } from "./Tooltip.svelte";

export { ICON_NAMES, ICONS, iconFor, isIconName, type IconName } from "./icons";
export type { MenuEntry, MenuHeading, MenuItem, MenuSeparator } from "./menu";
export type { Placement } from "./position";
export { tooltip, type TooltipOptions } from "./tooltip";
export { isMacPlatform } from "./platform";
