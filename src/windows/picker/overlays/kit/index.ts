// Shelf layer kit (W2-E). Build every overlay from these so geometry stays concentric:
//   LayerSurface — scrim + raised squircle surface (menu · popover · sheet · dialog)
//   LayerHeader  — title row with a concentric close circle
//   LayerItem    — 32 px capsule row (menu item, list entry)
//   LayerFooter  — keyboard hints + capsule buttons
// See docs/picker.md «Примитивы слоёв».
export { default as LayerSurface } from "./LayerSurface.svelte";
export { default as LayerHeader } from "./LayerHeader.svelte";
export { default as LayerItem } from "./LayerItem.svelte";
export { default as LayerFooter } from "./LayerFooter.svelte";
