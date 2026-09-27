const MIN_PX = 9;

/** Shrinks the font of `node` until every `.fit` label inside it fits on one line; its parameter re-fits when the text changes. */
export function fit(node: HTMLElement, _key?: unknown) {
  const run = () => {
    node.style.fontSize = "";
    const labels = [...node.querySelectorAll<HTMLElement>(".fit")];
    let px = parseFloat(getComputedStyle(node).fontSize);
    while (px > MIN_PX && labels.some((l) => l.scrollWidth > l.clientWidth)) node.style.fontSize = `${--px}px`;
  };
  const observer = new ResizeObserver(run);
  observer.observe(node);
  run();
  return {
    update: run,
    destroy: () => observer.disconnect(),
  };
}
