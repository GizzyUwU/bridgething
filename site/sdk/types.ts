export type SurfaceDocs = {
  version: string;
  surfaces: Surface[];
  types: Record<string, TypeDoc>;
};

export type Surface = {
  name: string;
  title: string;
  description?: string;
  events: Method[];
  requests: Method[];
  commands: Method[];
  handlers: Method[];
};

export type Method = {
  method: string;
  description?: string;
  payload?: string;
  payload_ref?: string;
  response?: string;
  error?: string;
  response_event?: string;
};

export type TypeDoc = StructType | EnumType;

export type StructType = {
  kind: 'struct';
  description?: string;
  fields: Field[];
};

export type EnumType = {
  kind: 'enum';
  description?: string;
  tag?: string;
  content?: string;
  variants: Variant[];
};

export type Field = {
  name: string;
  type: string;
  optional?: boolean;
  type_ref?: string;
  description?: string;
};

export type Variant = {
  name: string;
  payload?: string;
  payload_ref?: string;
  description?: string;
};

export const METHOD_GROUPS = [
  { key: 'requests', label: 'Requests', blurb: 'you ask, the daemon answers. await the tagged result and check .ok' },
  {
    key: 'commands',
    label: 'Commands',
    blurb: 'fire-and-forget. the promise resolves once the daemon has taken the message',
  },
  {
    key: 'events',
    label: 'Events',
    blurb: 'the daemon pushes these unprompted. subscribing returns an unsubscribe function',
  },
  { key: 'handlers', label: 'Request handlers', blurb: 'the daemon asks, your webapp answers through a typed handle' },
] as const satisfies ReadonlyArray<{
  key: keyof Surface & ('events' | 'requests' | 'commands' | 'handlers');
  label: string;
  blurb: string;
}>;
