<script lang="ts">
  interface Props {
    checked: boolean;
    onChange?: (checked: boolean) => void;
    disabled?: boolean;
    ariaLabel?: string;
  }
  let { checked, onChange, disabled = false, ariaLabel }: Props = $props();

  // Only animate the knob's liquid squash AFTER a real interaction — never on
  // the initial render (no page-load choreography).
  let touched = $state(false);
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={ariaLabel}
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
    width: 40px;
    height: 22px;
    flex: 0 0 auto;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--color-toggle-track);
    cursor: default;
    transition: background-color var(--dur-base) var(--ease-out-soft);
  }
  .toggle--on {
    background: var(--color-ink);
  }
  .toggle:disabled {
    opacity: 0.45;
  }
  .toggle__knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 18px;
    height: 18px;
    border-radius: 999px;
    background: var(--color-paper);
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.2);
    transform-origin: center;
    /* Resting / non-interactive moves use a liquid settle. */
    transition: transform var(--dur-base) var(--ease-liquid);
  }
  .toggle--on .toggle__knob {
    transform: translateX(18px);
  }
  /* Dark: the white knob would vanish on the near-white (ink) ON track, so the
     knob flips to read against each track — a light dot on the dark OFF track, a
     dark dot on the near-white ON track. */
  :global([data-theme="dark"]) .toggle__knob {
    background: var(--color-ink-2);
  }
  :global([data-theme="dark"]) .toggle--on .toggle__knob {
    background: var(--color-page);
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
      transform: translateX(11px) scaleX(1.18);
    }
    100% {
      transform: translateX(18px) scaleX(1);
    }
  }
  @keyframes knob-off {
    0% {
      transform: translateX(18px) scaleX(1);
    }
    55% {
      transform: translateX(6px) scaleX(1.18);
    }
    100% {
      transform: translateX(0) scaleX(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .toggle--anim[data-on="true"] .toggle__knob,
    .toggle--anim[data-on="false"] .toggle__knob {
      animation: none;
    }
  }
</style>
