import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "../../app.css";
import { mount } from "svelte";
import { initAppearance } from "$lib/appearance";
import Editor from "./Editor.svelte";

// Editor window (ARCHITECTURE §18.9; W6-C). Built on demand by Rust (W6-B), destroyed on close.
// Paint the mirrored theme/locale before mount so the first frame is already correct;
// the authoritative Settings.general arrives right after (settings_get).
initAppearance();

const app = mount(Editor, { target: document.getElementById("app")! });

export default app;
