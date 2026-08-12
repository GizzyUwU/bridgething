import type { ConfigField } from '@bridgething/companion-types';
import { Field, Segmented, Switch } from '@bridgething/ui';
import type { VNode } from 'preact';
import { useState } from 'preact/hooks';

import { Icon } from '../lib/icons.tsx';

export function ConfigEditor({
  field,
  value,
  onCommit,
  onReset,
}: {
  field: ConfigField;
  value: string;
  onCommit: (next: string) => void;
  onReset: () => void;
}): VNode {
  return (
    <div class="border border-rule bg-screen px-4 py-3">
      <div class="mb-2 flex items-center justify-between gap-3">
        <span class="font-mono text-eyebrow tracking-[0.18em] text-muted uppercase">{field.label}</span>
        {field.defaultValue !== null ? (
          <button
            type="button"
            class="flex shrink-0 items-center gap-1 font-mono text-eyebrow text-accent uppercase transition-colors hover:text-off-white focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2"
            onClick={onReset}>
            <Icon name="undo" size={11} />
            reset
          </button>
        ) : null}
      </div>
      <Control key={`${field.key}:${value}`} field={field} value={value} onCommit={onCommit} />
      {constraints(field) ? <p class="mt-2 font-mono text-hint text-dim">{constraints(field)}</p> : null}
    </div>
  );
}

function Control({
  field,
  value,
  onCommit,
}: {
  field: ConfigField;
  value: string;
  onCommit: (next: string) => void;
}): VNode {
  const [draft, setDraft] = useState(value);

  switch (field.kind) {
    case 'boolean':
      return (
        <div class="flex items-center justify-between gap-3">
          <span class="text-row text-off-white">{value === 'true' ? 'enabled' : 'disabled'}</span>
          <Switch checked={value === 'true'} label={field.label} onChange={next => onCommit(next ? 'true' : 'false')} />
        </div>
      );
    case 'enum':
      return field.choices.length === 0 ? (
        <p class="text-hint text-muted">this field declares no choices</p>
      ) : (
        <Segmented
          class="max-w-full flex-wrap"
          options={field.choices}
          value={value}
          label={field.label}
          size="sm"
          onChange={onCommit}
        />
      );
    case 'number':
      return <Field value={draft} onInput={setDraft} onCommit={onCommit} placeholder={field.defaultValue ?? '0'} />;
    case 'secret':
      return <Field type="password" value={draft} onInput={setDraft} onCommit={onCommit} placeholder="not set" />;
    case 'string':
      return <Field value={draft} onInput={setDraft} onCommit={onCommit} placeholder={field.defaultValue ?? ''} />;
  }
}

function constraints(field: ConfigField): string | null {
  const parts: string[] = [];
  if (field.min !== null) parts.push(`min ${field.min}`);
  if (field.max !== null) parts.push(`max ${field.max}`);
  if (field.step !== null) parts.push(`step ${field.step}`);
  if (field.minLength !== null) parts.push(`at least ${field.minLength} characters`);
  if (field.maxLength !== null) parts.push(`at most ${field.maxLength} characters`);
  if (field.pattern !== null) parts.push(field.pattern);
  return parts.length > 0 ? parts.join(' · ') : null;
}
