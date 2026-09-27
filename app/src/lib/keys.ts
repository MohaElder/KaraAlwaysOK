/** True while an input method (Japanese, Chinese, Korean…) is still composing the text this key belongs to. */
export const composing = (e: KeyboardEvent) => e.isComposing || e.keyCode === 229;
