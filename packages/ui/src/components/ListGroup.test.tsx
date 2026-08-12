import { describe, expect, test } from 'bun:test';
import type { VNode } from 'preact';

import { ListGroup } from './ListGroup.tsx';

type Slot = VNode<{ children?: unknown }>;

function slots(group: VNode): Slot[] {
  return (group.props as { children: Slot[] }).children;
}

function contentsOf(slot: Slot): unknown[] {
  const children = slot.props.children;
  return Array.isArray(children) ? children : [children];
}

function dividerOf(slot: Slot): VNode<{ class?: string }> | null {
  return contentsOf(slot)[0] as VNode<{ class?: string }> | null;
}

describe('ListGroup', () => {
  test('draws a hairline between adjacent children and never after the last', () => {
    const group = ListGroup({ children: [<i />, <b />, <u />] });
    const rendered = slots(group);

    expect(rendered).toHaveLength(3);
    expect(dividerOf(rendered[0]!)).toBeNull();
    expect(dividerOf(rendered[1]!)?.props.class).toContain('bg-rule');
    expect(dividerOf(rendered[2]!)?.props.class).toContain('bg-rule');
  });

  test('a lone child gets no hairline at all', () => {
    const rendered = slots(ListGroup({ children: <i /> }));

    expect(rendered).toHaveLength(1);
    expect(dividerOf(rendered[0]!)).toBeNull();
  });

  test('nullish children drop out before the hairlines are placed', () => {
    const rendered = slots(ListGroup({ children: [null, <i />, undefined, false, <b />] }));

    expect(rendered).toHaveLength(2);
    expect(dividerOf(rendered[0]!)).toBeNull();
    expect(dividerOf(rendered[1]!)?.props.class).toContain('bg-rule');
  });

  test('the child itself follows its hairline, in source order', () => {
    const first = <i />;
    const second = <b />;
    const rendered = slots(ListGroup({ children: [first, second] }));

    expect(contentsOf(rendered[0]!)[1]).toBe(first);
    expect(contentsOf(rendered[1]!)[1]).toBe(second);
  });

  test('an empty group renders no hairlines', () => {
    expect(slots(ListGroup({ children: [null, undefined] }))).toHaveLength(0);
  });
});
