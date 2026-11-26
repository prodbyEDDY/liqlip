import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "../../app.css";
import { mount } from "svelte";
import { initAppearance } from "$lib/appearance";
import Shots from "./Shots.svelte";

// Thumbnail stack after a shot — QAO (ARCHITECTURE §18.8; W6-E). Built on demand by Rust (W6-B).
// Paint the mirrored theme/locale before mount so the first frame is already correct;
// the authoritative Settings.general arrives right after (settings_get).
initAppearance();

const app = mount(Shots, { target: document.getElementById("app")! });

export default app;
