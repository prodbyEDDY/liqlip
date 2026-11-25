<script lang="ts">
  // On/off switch: a capsule track with a circular knob inset 2 px (concentric by
  // construction). `id` and `describedby` let a settings row label and describe it.
  interface Props {
    checked: boolean;
    onChange?: (checked: boolean) => void;
    disabled?: boolean;
    ariaLabel?: string;
    id?: string;
    describedby?: string;
  }
  let { checked, onChange, disabled = false, ariaLabel, id, describedby }: Props = $props();

  // Only animate the knob's liquid squash AFTER a real interaction — never on
  // the initial render (no page-load choreography).
  let touched = $state(false);
</script>

<button
  type="button"
  role="switch"
  {id}
  aria-checked={checked}
  aria-label={ariaLabel}
  aria-describedby={describedby}
  class="toggle"
  class:toggle--on={checked}
  class:toggle--anim={touched}
  data-on={checked}
  {disabled}
  onclick={() => {
    touched = true;
    onChange?.(!checked);
  }}
>
  <span class="toggle__knob"></span>
</button>

<style>
  .toggle {
    position: relative;
    flex: 0 0 auto;
    width: 40px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--color-toggle-track);
    transition: background-color var(--dur-base) var(--ease-out-soft);
  }
  .toggle:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-ink) 28%, transparent);
  }
  .toggle--on,
  .toggle--on:hover:not(:disabled) {
    background: var(--color-ink);
  }
  .toggle--on:hover:not(:disabled) {
    background: var(--color-ink-hover);
  }
  .toggle:disabled {
    opacity: 0.45;
  }
  .toggle__knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 20px;
    height: 20px;
    border-radius: 999px;
    background: #ffffff; /* light: a white knob on both tracks */
    box-shadow: var(--shadow-on-lift);
    transform-origin: center;
    transition: transform var(--dur-base) var(--ease-liquid);
  }
  .toggle--on .toggle__knob {
    transform: translateX(16px);
  }
  /* Dark: a light knob on the dark OFF track, a dark knob on the near-white ON track. */
  :global([data-theme="dark"]) .toggle__knob {
    background: var(--color-ink-2);
  }
  :global([data-theme="dark"]) .toggle--on .toggle__knob {
    background: var(--color-paper);
  }
  .toggle:active:not(:disabled) .toggle__knob {
    width: 23px;
  }
  .toggle--on:active:not(:disabled) .toggle__knob {
    transform: translateX(13px);
  }
  /* On a real toggle, the knob stretches toward travel then settles (liquid). */
  .toggle--anim[data-on="true"] .toggle__knob {
    animation: knob-on var(--dur-base) var(--ease-out-soft);
  }
  .toggle--anim[data-on="false"] .toggle__knob {
    animation: knob-off var(--dur-base) var(--ease-out-soft);
  }
  @keyframes knob-on {
    0% {
      transform: translateX(0) scaleX(1);
    }
    55% {
      transform: translateX(10px) scaleX(1.16);
    }
    100% {
      transform: translateX(16px) scaleX(1);
    }
  }
  @keyframes knob-off {
    0% {
      transform: translateX(16px) scaleX(1);
    }
    55% {
      transform: translateX(6px) scaleX(1.16);
    }
    100% {
      transform: translateX(0) scaleX(1);
    }
  }
</style>
