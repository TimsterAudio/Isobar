export interface View {
  el: HTMLElement;
  destroy(): void;
  /** Redraw charts, e.g. after a theme change. */
  rerender?(): void;
}
