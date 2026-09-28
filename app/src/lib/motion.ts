import type { TransitionConfig } from "svelte/transition";

export const DURATION = 240;

/** cubic-bezier(.2, .7, .2, 1), the one easing used everywhere. */
export function ease(x: number): number {
  const at = (a: number, b: number, t: number) => 3 * a * t * (1 - t) ** 2 + 3 * b * t * t * (1 - t) + t ** 3;
  let lo = 0, hi = 1, t = x;
  for (let i = 0; i < 20; i++) {
    t = (lo + hi) / 2;
    if (at(0.2, 0.2, t) < x) lo = t;
    else hi = t;
  }
  return at(0.7, 1, t);
}

const reducedMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

export function fade(_node: Element): TransitionConfig {
  return { duration: DURATION, easing: ease, css: (t) => `opacity:${t}` };
}

/** A short slide from (x, y) pixels away, with a fade; only the fade under reduced motion. */
export function slide(node: Element, { x = 0, y = 8 }: { x?: number; y?: number } = {}): TransitionConfig {
  if (reducedMotion()) return fade(node);
  return { duration: DURATION, easing: ease, css: (t, u) => `opacity:${t};transform:translate(${u * x}px,${u * y}px)` };
}

/** Opens or closes the element's height with a fade, so what's around it moves along; only the fade under reduced motion. */
export function unfold(node: Element): TransitionConfig {
  if (reducedMotion()) return fade(node);
  const style = getComputedStyle(node);
  const sizes = ["height", "padding-top", "padding-bottom", "margin-top", "margin-bottom"].map((p) => [p, parseFloat(style.getPropertyValue(p))] as const);
  return { duration: DURATION, easing: ease, css: (t) => `opacity:${t};overflow:hidden;${sizes.map(([p, v]) => `${p}:${t * v}px`).join(";")}` };
}

/** A fade out that lifts the element out of the flow where it stands, so what replaces it doesn't jump. */
export function fadeAway(node: HTMLElement): TransitionConfig {
  const { offsetTop: top, offsetLeft: left, offsetWidth: width } = node;
  return { duration: DURATION, easing: ease, css: (t) => `position:absolute;top:${top}px;left:${left}px;width:${width}px;opacity:${t}` };
}
