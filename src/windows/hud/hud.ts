import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "../../app.css";
import { mount } from "svelte";
import { initAppearance } from "$lib/appearance";
import Hud from "./Hud.svelte";

// Paint the mirrored theme/locale before mount so the first frame is already correct;
// the authoritative Settings.general arrives right after (settings_get).
initAppearance();

const app = mount(Hud, { target: document.getElementById("app")! });

export default app;
