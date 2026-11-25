<script lang="ts" module>
  import type { IconName } from "$lib/icons";
  export interface MenuItemDef {
    key: string;
    label: string;
    icon: IconName;
  }
</script>

<script lang="ts">
  // Ported from Liquid Tools `Menu.svelte` without the launcher-specific parts
  // (tab strip, account chip, frameless window drag): a vertical list of
  // MenuItems with one selected entry — the settings sidebar uses it.
  import MenuItem from "./MenuItem.svelte";

  interface Props {
    items: MenuItemDef[];
    selectedItem: string;
    onSelectItem?: (key: string) => void;
    ariaLabel?: string;
  }

  let { items, selectedItem, onSelectItem, ariaLabel }: Props = $props();
</script>

<nav class="menu" aria-label={ariaLabel}>
  {#each items as item (item.key)}
    <MenuItem
      icon={item.icon}
      label={item.label}
      selected={item.key === selectedItem}
      chevron={false}
      onclick={() => onSelectItem?.(item.key)}
    />
  {/each}
</nav>

<style>
  .menu {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
</style>
