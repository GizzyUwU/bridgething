import type { BridgeToGatewayMsg } from '@bridgething/lib';
import { decode } from '@msgpack/msgpack';

type Adapter = {
  on(callback: (event: AllAdapterEvent) => void): void;

  init(): Promise<void>;
  init(adapterName?: string | null): Promise<void>;

  scanOn(): void;
  scanOff(): void;

  disconnect(macAddress: string): void;

  // send(): Promise<void>;
};

type UpperAdapterEvent =
  | { type: 'Connected'; name: string; macAddress: string }
  | { type: 'Disconnected'; macAddress: string }
  | { type: 'Data'; macAddress: string; data: Uint8Array };
type AdapterEvent =
  | { type: 'connected'; name: string; macAddress: string }
  | { type: 'disconnected'; macAddress: string }
  | { type: 'data'; macAddress: string; data: Uint8Array };
type AllAdapterEvent = UpperAdapterEvent | AdapterEvent;

type ParsedDataEvent = { type: 'data'; macAddress: string; data: BridgeToGatewayMsg };
type ParsedAdapterEvent<T extends AdapterEvent = AdapterEvent> = T['type'] extends 'data' ? ParsedDataEvent : T;

type EventByType<T extends AdapterEvent['type']> = Extract<AdapterEvent, { type: T }>;
type SimpleEventCallback = (event: AdapterEvent) => void;
type KeyedAdapterEvent<K extends AdapterEvent['type']> = Omit<EventByType<K>, 'type'>;
type KeyedEventCallback<K extends string> = K extends AdapterEvent['type']
  ? K extends 'data'
    ? (event: Omit<ParsedDataEvent, 'type'>) => void
    : (event: KeyedAdapterEvent<K>) => void
  : never;
type EventCallbacks<Keys extends AdapterEvent['type'] | 'all' = AdapterEvent['type'] | 'all'> = {
  [K in Keys]: (K extends 'all' ? SimpleEventCallback : KeyedEventCallback<K>)[];
};

class BridgethingGateway {
  private readonly callbacks: EventCallbacks = {
    connected: [],
    disconnected: [],
    data: [],
    all: [],
  };
  constructor(private readonly adapter: Adapter) {
    adapter.on(allEvent => void this.handleEvent(allEvent));
  }

  init(adapterName?: string | null) {
    return this.adapter.init(adapterName);
  }

  on<K extends Lowercase<AdapterEvent['type']>>(type: K, callback: KeyedEventCallback<K>): void;
  on(callback: SimpleEventCallback): void;
  on<K extends Lowercase<AdapterEvent['type']>>(...params: [K, KeyedEventCallback<K>] | [SimpleEventCallback]) {
    if (typeof params[0] === 'string' && typeof params[1] === 'function')
      // this is a safe typecast bc of the generic drilling
      (this.callbacks[params[0]] as KeyedEventCallback<K>[]).push(params[1]);
    else if (typeof params[0] === 'function') this.callbacks.all.push(params[0]);
    else throw new Error('invalid callback.');
  }

  private handleEvent(allEvent: AllAdapterEvent) {
    // console.log('new event: ', allEvent);

    let data;
    if (allEvent.type === 'data' || allEvent.type === 'Data') data = decode(allEvent.data);
    const event = { ...allEvent, type: allEvent.type.toLowerCase(), data } as ParsedAdapterEvent;
    // console.log('decoded event: ', event);

    this.callbacks.all.map(callback => callback(event));

    const { type: _, ...taggedEvent } = event;
    if (Array.isArray(this.callbacks[event.type]))
      this.callbacks[event.type].map(callback => callback(taggedEvent as never));
  }
}

export { BridgethingGateway, type Adapter, type AdapterEvent, type KeyedAdapterEvent };
