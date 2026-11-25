<script lang="ts">
  // Context menu of a pill: a folder — Rename (inline) · Delete folder… (confirmation, SPEC
  // §3.9); the Snippets pill — New snippet… (SPEC §3.10) · Manage in Settings.
  import { t } from "$lib/i18n/index.svelte";
  import { settingsOpen } from "$lib/ipc/commands";
  import { errorMessage } from "$lib/ipc/errors";
  import MenuLayer, { type MenuEntry } from "./MenuLayer.svelte";
  import { overlays, type LayerProps } from "../overlays.svelte";
  import { shelf } from "../shelf.svelte";
  import { toasts } from "../toasts.svelte";
  import { confirmFolderDelete } from "./layerActions";

  let { layer, close }: LayerProps = $props();

  const folderId = $derived((layer.ctx.folderId as number | undefined) ?? null);
  const snippets = $derived(layer.ctx.snippets === true);

  const items: MenuEntry[] = $derived(
    snippets
      ? [
          { id: "new", label: t("picker.menu.snippetNew"), icon: "plus", run: () => overlays.open("snippet", { mode: "new" }) },
          {
            id: "settings",
            label: t("picker.menu.snippetSettings"),
            icon: "settings",
            run: () => void settingsOpen("snippets").catch((e) => toasts.error(errorMessage(e))),
          },
        ]
      : [
          {
            id: "rename",
            label: t("picker.folder.rename"),
            icon: "pen",
            run: () => {
              shelf.renamingFolder = folderId;
            },
          },
          {
            id: "delete",
            label: t("picker.folder.delete"),
            icon: "trash",
            danger: true,
            gapBefore: true,
            run: () => {
              if (folderId !== null) confirmFolderDelete(folderId);
            },
          },
        ],
  );
  const label = $derived(snippets ? t("picker.pill.snippets") : t("picker.folder.menu", { name: shelf.folderName(folderId) }));
</script>

<MenuLayer {layer} {close} {items} {label} />
