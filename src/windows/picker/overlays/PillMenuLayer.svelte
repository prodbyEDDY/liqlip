<script lang="ts">
  // Context menu of a folder pill: Rename (inline) · Delete folder… (confirmation, SPEC §3.9).
  import { t } from "$lib/i18n/index.svelte";
  import MenuLayer, { type MenuEntry } from "./MenuLayer.svelte";
  import { overlays, type LayerProps } from "../overlays.svelte";
  import { shelf } from "../shelf.svelte";

  let { layer, close }: LayerProps = $props();

  const folderId = $derived(layer.ctx.folderId as number);
  const items: MenuEntry[] = $derived([
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
      run: () => {
        overlays.open("confirm", { folderId });
      },
    },
  ]);
</script>

<MenuLayer {layer} {close} {items} label={t("picker.folder.menu", { name: shelf.folderName(folderId) })} />
