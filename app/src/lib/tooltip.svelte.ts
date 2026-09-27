class TipState {
  current = $state<{ text: string; rect: DOMRect } | null>(null);
}

export const tipState = new TipState();

/** Names an icon-only control: shows `text` as a tooltip after the pointer rests on it. */
export function tip(node: HTMLElement, text: string) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let shown = false;
  const labels = !node.hasAttribute("aria-label");
  const show = () => {
    shown = true;
    tipState.current = { text, rect: node.getBoundingClientRect() };
  };
  const hide = () => {
    clearTimeout(timer);
    shown = false;
    tipState.current = null;
  };
  const enter = () => {
    clearTimeout(timer);
    timer = setTimeout(show, 450);
  };
  if (labels) node.setAttribute("aria-label", text);
  node.addEventListener("pointerenter", enter);
  node.addEventListener("pointerleave", hide);
  node.addEventListener("pointerdown", hide);
  return {
    update(next: string) {
      text = next;
      if (labels) node.setAttribute("aria-label", next);
      if (shown) show();
    },
    destroy() {
      hide();
      node.removeEventListener("pointerenter", enter);
      node.removeEventListener("pointerleave", hide);
      node.removeEventListener("pointerdown", hide);
    },
  };
}
