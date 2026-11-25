<script lang="ts">
  // W1 stand-in for the actions menu (Ctrl+K, right click, ⋯): Paste · Copy · Favorite ·
  // Delete, all through `runAction`. W2-C registers the full searchable menu (SPEC §3.7) over
  // this kind: `overlays.register("actions", ActionsMenu)`.
  import { t } from "$lib/i18n/index.svelte";
  import type { Snippet } from "$lib/ipc/types";
  import MenuLayer, { type MenuEntry } from "./MenuLayer.svelte";
  import type { LayerProps } from "../overlays.svelte";
  import { feed } from "../feed.svelte";
  import { copySnippet, pasteSnippet, runAction } from "../actions.svelte";

  let { layer, close }: LayerProps = $props();

  const items: MenuEntry[] = $derived.by(() => {
    const ids = layer.ctx.ids ?? [];
    const snippet = (layer.ctx.snippet as Snippet | null | undefined) ?? null;
    if (snippet) {
      return [
        { id: "paste", label: t("picker.action.paste"), icon: "paste", keys: "↵", run: () => void pasteSnippet(snippet, false) },
        { id: "copy", label: t("picker.action.copy"), icon: "copy", keys: "Ctrl ↵", run: () => void copySnippet(snippet) },
      ];
    }
    const clips = ids.map((id) => feed.clip(id)).filter((c) => c !== undefined);
    const allFav = clips.length > 0 && clips.every((c) => c.favorite);
    return [
      { id: "paste", label: t("picker.action.paste"), icon: "paste", keys: "↵", run: () => void runAction("paste", ids) },
      { id: "copy", label: t("picker.action.copy"), icon: "copy", keys: "Ctrl ↵", run: () => void runAction("copy", ids) },
      {
        id: "favorite",
        label: t(allFav ? "picker.action.unfavorite" : "picker.action.favorite"),
        icon: "heart",
        keys: "Ctrl D",
        run: () => void runAction("toggleFavorite", ids),
      },
      {
        id: "delete",
        label: ids.length > 1 ? t("picker.action.deleteMany", { n: ids.length }) : t("picker.action.delete"),
        icon: "trash",
        keys: "Del",
        danger: true,
        run: () => void runAction("delete", ids),
      },
    ];
  });
</script>

<MenuLayer {layer} {close} {items} label={t("picker.action.menu")} />
