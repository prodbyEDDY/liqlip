// Built-in W1 layers. W2-C overrides `actions`, `preview` and `stack` by registering its own
// components after this (e.g. in its own module imported by `Picker.svelte`).
import { overlays } from "../overlays.svelte";
import ActionsLayer from "./ActionsLayer.svelte";
import PreviewLayer from "./PreviewLayer.svelte";
import StackLayer from "./StackLayer.svelte";
import ConfirmLayer from "./ConfirmLayer.svelte";
import PillMenuLayer from "./PillMenuLayer.svelte";

export function registerDefaultLayers(): void {
  overlays.register("actions", ActionsLayer);
  overlays.register("preview", PreviewLayer);
  overlays.register("stack", StackLayer);
  overlays.register("confirm", ConfirmLayer);
  overlays.register("pillMenu", PillMenuLayer);
}
