import { Fragment, toChildArray, type ComponentChildren, type VNode } from 'preact';

import { cx } from '../cx.ts';

export function ListGroup({ class: className, children }: { class?: string; children: ComponentChildren }): VNode {
  const items = toChildArray(children);

  return (
    <div class={cx('flex flex-col border border-rule bg-screen', className)}>
      {items.map((child, index) => (
        <Fragment key={index}>
          {index > 0 ? <span class="h-px shrink-0 bg-rule" /> : null}
          {child}
        </Fragment>
      ))}
    </div>
  );
}
