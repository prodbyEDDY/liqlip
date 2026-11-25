import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "node:path";

// Tauri exposes this when running on a physical mobile device; harmless on desktop.
const host = process.env.TAURI_DEV_HOST;

const root = import.meta.dirname;

// https://vite.dev/config/  +  https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [svelte(), tailwindcss()],

  resolve: {
    alias: {
      $lib: resolve(root, "src/lib"),
    },
  },

  // --- Vite options tailored for Tauri development ---
  // Tauri controls the terminal output, so don't let Vite clear it.
  clearScreen: false,
  server: {
    // Tauri expects a fixed, known port; fail rather than silently pick another.
    // 1430 (not 1420) so Liqlip and Liquid Tools can run side by side.
    port: 1430,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1431 } : undefined,
    watch: {
      // The Tauri CLI watches the Rust sources itself.
      ignored: ["**/src-tauri/**"],
    },
  },
  // Env vars starting with these prefixes are exposed to the client via import.meta.env.
  envPrefix: ["VITE_", "TAURI_ENV_*"],

  build: {
    // Windows uses WebView2 (Chromium).
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "es2022",
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // Multi-page app: one minimal entry per Tauri window. Each window loads only its own code.
    rollupOptions: {
      input: {
        picker: resolve(root, "picker.html"),
        settings: resolve(root, "settings.html"),
        hud: resolve(root, "hud.html"),
      },
    },
  },
});
