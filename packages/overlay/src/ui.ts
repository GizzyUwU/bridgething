import css from './style.css?inline';

export type OverlayRoot = {
  top: HTMLElement;
  toasts: HTMLElement;
  layer: HTMLElement;
};

export function mountRoot(container?: HTMLElement): OverlayRoot {
  const host = document.createElement('bridgething-overlay');
  const shadow = host.attachShadow({ mode: 'closed' });
  const style = document.createElement('style');
  style.textContent = css;
  const top = el('div', 'top');
  const toasts = el('div', 'toasts');
  const layer = el('div', '');
  shadow.append(style, top, toasts, layer);
  if (container) {
    host.style.position = 'absolute';
    container.appendChild(host);
  } else {
    (document.body ?? document.documentElement).appendChild(host);
  }
  return { top, toasts, layer };
}

export function el(tag: string, className: string, text?: string): HTMLElement {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}
