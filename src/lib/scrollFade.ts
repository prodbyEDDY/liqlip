// `use:scrollFade` — a vertical scroller fades the edge(s) behind which content continues
// (design-system §7: nothing is cut at an invisible line). Sets `data-fade` to "start",
// "end" or "both" (removed when everything fits); the mask itself lives in app.css
// (`[data-fade]`, size `--fade`, default 16 px). Follows scrolling, resizes of the scroller
// and changes of its content.

/** Which edges hide content: `null` when it all fits. Pure (tests/scrollFade.test.mjs). */
export function fadeEdges(scrollTop: number, clientHeight: number, scrollHeight: number): "start" | "end" | "both" | null {
  const start = scrollTop > 1;
  const end = scrollTop + clientHeight < scrollHeight - 1;
  return start && end ? "both" : start ? "start" : end ? "end" : null;
}

export function scrollFade(node: HTMLElement): { destroy(): void } {
  let frame = 0;
  const sync = () => {
    frame = 0;
    const edges = fadeEdges(node.scrollTop, node.clientHeight, node.scrollHeight);
    if (edges) node.dataset.fade = edges;
    else delete node.dataset.fade;
  };
  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(sync);
  };
  sync();
  node.addEventListener("scroll", schedule, { passive: true });
  const resize = new ResizeObserver(schedule);
  resize.observe(node);
  const content = new MutationObserver(schedule);
  content.observe(node, { childList: true, subtree: true, characterData: true });
  return {
    destroy() {
      cancelAnimationFrame(frame);
      node.removeEventListener("scroll", schedule);
      resize.disconnect();
      content.disconnect();
    },
  };
}
