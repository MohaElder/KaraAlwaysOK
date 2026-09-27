class TipState {
  current = $state<{ text: string; rect: DOMRect } | null>(null);
}

export const tipState = new TipState();

/** Names an icon-only control: shows `text` as a tooltip after the pointer rests on it. */
export function tip(node: HTMLElement, text: string) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const hide = () => {
    clearTimeout(timer);
    tipState.current = null;
  };
  const enter = () => {
    clearTimeout(timer);
    timer = setTimeout(() => (tipState.current = { text, rect: node.getBoundingClientRect() }), 450);
  };
  if (!node.hasAttribute("aria-label")) node.setAttribute("aria-label", text);
  node.addEventListener("pointerenter", enter);
  node.addEventListener("pointerleave", hide);
  node.addEventListener("pointerdown", hide);
  return {
    update(next: string) {
      text = next;
      node.setAttribute("aria-label", next);
    },
    destroy() {
      hide();
      node.removeEventListener("pointerenter", enter);
      node.removeEventListener("pointerleave", hide);
      node.removeEventListener("pointerdown", hide);
    },
  };
}
