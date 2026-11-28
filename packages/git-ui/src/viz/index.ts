// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

// The graphics of the Git views (wave 8, strand E of the product's wave 8): branch map, commit
// calendar, columns, trend, bars, time axis, treemap, radar matrix, stack diagram, age strip and
// sparkline. Plain SVG and HTML with no dependency; each has a text form for screen readers
// and takes its colours from the tokens, so light and dark follow the page. Additive: nothing
// that existed before changes.

export { default as ActivityBar } from "./ActivityBar.svelte";
export { default as AgeStrip, type AgeItem } from "./AgeStrip.svelte";
export { default as DiffMinimap } from "../diff/DiffMinimap.svelte";
export { default as BarList, type BarItem } from "./BarList.svelte";
export { default as BranchMap } from "./BranchMap.svelte";
export { default as CalendarGrid } from "./CalendarGrid.svelte";
export { default as ColumnChart } from "./ColumnChart.svelte";
export { default as RadarMatrix, type RadarCellState, type RadarColumn, type RadarRow } from "./RadarMatrix.svelte";
export { default as Sparkline } from "./Sparkline.svelte";
export { default as StackDiagram, type StackNode, type StackPullState } from "./StackDiagram.svelte";
export { default as TimeAxis, type TimeMark } from "./TimeAxis.svelte";
export { default as Treemap, type TreemapCell } from "./Treemap.svelte";
export { default as TrendChart, type TrendPoint } from "./TrendChart.svelte";

export { describeLine, layoutBranchMap, pickTrunk, syntheticBranchMap } from "./branchmap";
export type { BranchMapInput, BranchMapLayout, MapCommit, MapLine, MapLink, MapRef, MapStation } from "./branchmap";
export { calendarCells, dayNumber, dayString, longDay, monthLabels, weekdayOf } from "./calendar";
export type { CalendarCell, CalendarDay } from "./calendar";
export { AGE_LABELS, AGE_LIMITS_DAYS, ageColor, ageStep, categoryColor, heatColor, heatLevels } from "./color";
export { dateTicks, placeMarks } from "./timeaxis";
export type { AxisMark, PlacedMark } from "./timeaxis";
export { squarify } from "./treemap";
export type { TreemapItem, TreemapRect } from "./treemap";
