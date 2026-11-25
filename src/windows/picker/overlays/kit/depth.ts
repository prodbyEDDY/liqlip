// Stacking of open layers: `OverlayHost` gives each layer its depth (0 = bottom) through this
// context, and `LayerSurface` lifts its scrim and surface by it — a dialog over Quick Look
// dims and blocks Quick Look, not just the grid.
export const LAYER_DEPTH = Symbol("layer-depth");
