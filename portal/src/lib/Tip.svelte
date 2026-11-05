<script lang="ts">
  // A small "?" that shows an explanation on hover or focus. Not a button, so
  // inside a <label> it doesn't take over the label's input.
  let { text }: { text: string } = $props();
  const id = `tip-${Math.random().toString(36).slice(2, 9)}`;
</script>

<span class="tip">
  <!-- Clicking it inside a <label> would focus the input instead; Escape hides it. -->
  <span class="i" tabindex="0" role="button" aria-label="What is this?" aria-describedby={id}
    onclick={(e) => e.preventDefault()} onkeydown={(e) => e.key === 'Escape' && e.currentTarget.blur()}>?</span>
  <span class="bubble" role="tooltip" {id}>{text}</span>
</span>

<style>
  .tip { position: relative; display: inline-flex; }
  .i {
    display: inline-grid; place-items: center; width: 16px; height: 16px; border-radius: 50%;
    border: 1px solid var(--control); color: var(--faint); font-size: 11px; font-weight: 600; line-height: 1; cursor: help;
  }
  .i:hover, .i:focus-visible { color: var(--text); border-color: var(--muted); outline: none; }
  .bubble {
    position: absolute; left: -8px; top: calc(100% + 8px); z-index: 5; width: max-content; max-width: 300px;
    padding: 8px 10px; border-radius: var(--radius-inner); background: var(--raised); color: var(--text);
    font-size: var(--fs-small); font-weight: 400; line-height: 1.45; white-space: normal;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.5); visibility: hidden; opacity: 0; transition: opacity 0.12s;
    pointer-events: none;
  }
  .tip:hover .bubble, .i:focus + .bubble { visibility: visible; opacity: 1; }
</style>
