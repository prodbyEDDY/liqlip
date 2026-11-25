import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default {
  // Enables TypeScript inside <script> / <style> blocks.
  preprocess: vitePreprocess(),
};
