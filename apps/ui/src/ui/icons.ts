// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// The semantic icon set (wave 14 design foundation). Components name icons by meaning ("commit",
// "environment"), never by Lucide file, so the set can change in one place. Lucide icons are ISC
// licensed (@lucide/svelte, https://lucide.dev); each import below pulls in one icon only.

import type { Component } from "svelte";

import Activity from "@lucide/svelte/icons/activity";
import Archive from "@lucide/svelte/icons/archive";
import ArrowDownToLine from "@lucide/svelte/icons/arrow-down-to-line";
import ArrowDownUp from "@lucide/svelte/icons/arrow-down-up";
import ArrowUpFromLine from "@lucide/svelte/icons/arrow-up-from-line";
import Bot from "@lucide/svelte/icons/bot";
import Check from "@lucide/svelte/icons/check";
import ChevronDown from "@lucide/svelte/icons/chevron-down";
import ChevronLeft from "@lucide/svelte/icons/chevron-left";
import ChevronRight from "@lucide/svelte/icons/chevron-right";
import ChevronUp from "@lucide/svelte/icons/chevron-up";
import CircleCheck from "@lucide/svelte/icons/circle-check";
import CircleExclamationPoint from "@lucide/svelte/icons/circle-exclamation-point";
import CircleQuestionMark from "@lucide/svelte/icons/circle-question-mark";
import Code from "@lucide/svelte/icons/code";
import Copy from "@lucide/svelte/icons/copy";
import Ellipsis from "@lucide/svelte/icons/ellipsis";
import EllipsisVertical from "@lucide/svelte/icons/ellipsis-vertical";
import ExternalLink from "@lucide/svelte/icons/external-link";
import Eye from "@lucide/svelte/icons/eye";
import EyeOff from "@lucide/svelte/icons/eye-off";
import File from "@lucide/svelte/icons/file";
import FileCheck from "@lucide/svelte/icons/file-check";
import FileText from "@lucide/svelte/icons/file-text";
import Folder from "@lucide/svelte/icons/folder";
import FolderOpen from "@lucide/svelte/icons/folder-open";
import Funnel from "@lucide/svelte/icons/funnel";
import GitBranch from "@lucide/svelte/icons/git-branch";
import GitCommitHorizontal from "@lucide/svelte/icons/git-commit-horizontal";
import GitCompare from "@lucide/svelte/icons/git-compare";
import GitGraph from "@lucide/svelte/icons/git-graph";
import GitMerge from "@lucide/svelte/icons/git-merge";
import GitPullRequest from "@lucide/svelte/icons/git-pull-request";
import Globe from "@lucide/svelte/icons/globe";
import GripVertical from "@lucide/svelte/icons/grip-vertical";
import Hexagon from "@lucide/svelte/icons/hexagon";
import Import from "@lucide/svelte/icons/import";
import Inbox from "@lucide/svelte/icons/inbox";
import Info from "@lucide/svelte/icons/info";
import KeyRound from "@lucide/svelte/icons/key-round";
import Layers from "@lucide/svelte/icons/layers";
import Library from "@lucide/svelte/icons/library";
import Lock from "@lucide/svelte/icons/lock";
import Minus from "@lucide/svelte/icons/minus";
import Monitor from "@lucide/svelte/icons/monitor";
import Moon from "@lucide/svelte/icons/moon";
import Network from "@lucide/svelte/icons/network";
import Pencil from "@lucide/svelte/icons/pencil";
import Play from "@lucide/svelte/icons/play";
import Plus from "@lucide/svelte/icons/plus";
import RadioTower from "@lucide/svelte/icons/radio-tower";
import Redo2 from "@lucide/svelte/icons/redo-2";
import RefreshCw from "@lucide/svelte/icons/refresh-cw";
import RotateCcwClock from "@lucide/svelte/icons/rotate-ccw-clock";
import ScanSearch from "@lucide/svelte/icons/scan-search";
import Search from "@lucide/svelte/icons/search";
import Send from "@lucide/svelte/icons/send";
import Server from "@lucide/svelte/icons/server";
import Settings from "@lucide/svelte/icons/settings";
import ShieldLock from "@lucide/svelte/icons/shield-lock";
import Sparkles from "@lucide/svelte/icons/sparkles";
import Star from "@lucide/svelte/icons/star";
import Sun from "@lucide/svelte/icons/sun";
import Tag from "@lucide/svelte/icons/tag";
import Trash from "@lucide/svelte/icons/trash";
import TriangleExclamationPoint from "@lucide/svelte/icons/triangle-exclamation-point";
import Undo2 from "@lucide/svelte/icons/undo-2";
import User from "@lucide/svelte/icons/user";
import Variable from "@lucide/svelte/icons/variable";
import Workflow from "@lucide/svelte/icons/workflow";
import X from "@lucide/svelte/icons/x";
import Zap from "@lucide/svelte/icons/zap";

/** Every semantic icon name, grouped: the "New…" set (`NEW_ICONS`), Git, then general UI. */
export const ICONS = {
  // The "New…" dialog and node chips (apps/ui/src/new/icons.ts).
  http: Globe,
  graphql: Hexagon,
  grpc: Network,
  websocket: ArrowDownUp,
  sse: RadioTower,
  collection: Library,
  folder: Folder,
  environment: Layers,
  mcp: Bot,
  import: Import,
  service: Server,
  diagram: Workflow,
  trace: Activity,
  spark: Sparkles,
  // Git and API history.
  branch: GitBranch,
  commit: GitCommitHorizontal,
  merge: GitMerge,
  pull: ArrowDownToLine,
  push: ArrowUpFromLine,
  sync: RefreshCw,
  stash: Archive,
  tag: Tag,
  compare: GitCompare,
  history: RotateCcwClock,
  bisect: ScanSearch,
  review: GitPullRequest,
  contract: FileCheck,
  "shield-secret": ShieldLock,
  graph: GitGraph,
  // General UI.
  add: Plus,
  close: X,
  more: Ellipsis,
  "more-vertical": EllipsisVertical,
  chevron: ChevronDown,
  "chevron-down": ChevronDown,
  "chevron-up": ChevronUp,
  "chevron-left": ChevronLeft,
  "chevron-right": ChevronRight,
  check: Check,
  warning: TriangleExclamationPoint,
  info: Info,
  error: CircleExclamationPoint,
  success: CircleCheck,
  help: CircleQuestionMark,
  search: Search,
  settings: Settings,
  undo: Undo2,
  redo: Redo2,
  "external-link": ExternalLink,
  minus: Minus,
  copy: Copy,
  delete: Trash,
  edit: Pencil,
  file: File,
  "file-text": FileText,
  "folder-open": FolderOpen,
  eye: Eye,
  "eye-off": EyeOff,
  lock: Lock,
  key: KeyRound,
  play: Play,
  send: Send,
  star: Star,
  filter: Funnel,
  grip: GripVertical,
  sun: Sun,
  moon: Moon,
  monitor: Monitor,
  inbox: Inbox,
  zap: Zap,
  user: User,
  code: Code,
  variable: Variable,
} satisfies Record<string, Component<Record<string, unknown>>>;

export type IconName = keyof typeof ICONS;

export const ICON_NAMES = Object.keys(ICONS) as IconName[];

export const isIconName = (name: unknown): name is IconName => typeof name === "string" && Object.hasOwn(ICONS, name);

/** The icon for a name from outside (extensions pass strings); unknown names get `fallback`. */
export function iconFor(name: string | undefined, fallback: IconName = "spark"): Component<Record<string, unknown>> {
  return isIconName(name) ? ICONS[name] : ICONS[fallback];
}
