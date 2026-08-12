import type { ConfigField } from '@bridgething/companion-types';

export function toWireConfigField(field: ConfigField): Record<string, unknown> {
  switch (field.kind) {
    case 'number':
      return {
        type: 'number',
        data: {
          key: field.key,
          label: field.label,
          min: field.min,
          max: field.max,
          step: field.step,
          default: field.defaultValue !== null ? Number(field.defaultValue) : undefined,
        },
      };
    case 'boolean':
      return {
        type: 'boolean',
        data: {
          key: field.key,
          label: field.label,
          default: field.defaultValue !== null ? field.defaultValue === 'true' : undefined,
        },
      };
    case 'enum':
      return {
        type: 'enum',
        data: { key: field.key, label: field.label, choices: field.choices, default: field.defaultValue },
      };
    case 'secret':
    case 'string':
      return {
        type: field.kind === 'secret' ? 'secret' : 'string',
        data: {
          key: field.key,
          label: field.label,
          pattern: field.pattern,
          minLength: field.minLength,
          maxLength: field.maxLength,
          default: field.defaultValue,
        },
      };
  }
}
