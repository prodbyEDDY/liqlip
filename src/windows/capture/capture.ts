import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "../../app.css";
import { mount } from "svelte";
import { initAppearance } from "$lib/appearance";
import Capture from "./Capture.svelte";

// Capture overlay (ARCHITECTURE §18.7; W6-D). Pre-created hidden in `setup()`, loaded once.
// Paint the mirrored theme/locale before mount so the first frame is already correct;
// the authoritative Settings.general arrives right after (settings_get).
initAppearance();

const app = mount(Capture, { target: document.getElementById("app")! });

export default app;
