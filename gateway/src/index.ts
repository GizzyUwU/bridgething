import type { BridgeToGatewayMsg, GatewayToBridgeMsg } from '@bridgething/lib';
import { Logger, LogLevel } from '@bridgething/lib';
import { decode, encode } from '@msgpack/msgpack';

type AdapterCallback = (event: AllAdapterEvent) => void;
type Adapter = {
  on(callback: AdapterCallback): void;

  init(): Promise<void>;

  scanOn(): Promise<void>;
  scanOff(): Promise<void>;

  disconnect(deviceId: string): Promise<void>;

  send(deviceId: string, message: Uint8Array): Promise<void>;
};

type UpperAdapterEvent =
  | { type: 'Connected'; name: string; deviceId: string }
  | { type: 'Disconnected'; deviceId: string }
  | { type: 'Data'; deviceId: string; data: Uint8Array };
type AdapterEvent =
  | { type: 'connected'; name: string; deviceId: string }
  | { type: 'disconnected'; deviceId: string }
  | { type: 'data'; deviceId: string; data: Uint8Array };
type AllAdapterEvent = UpperAdapterEvent | AdapterEvent;

type ParsedDataEvent = { type: 'data'; deviceId: string; data: BridgeToGatewayMsg };
type ParsedAdapterEvent<T extends AdapterEvent = AdapterEvent> = T['type'] extends 'data' ? ParsedDataEvent : T;

type EventByType<T extends AdapterEvent['type']> = Extract<AdapterEvent, { type: T }>;
type KeyedAdapterEvent<K extends AdapterEvent['type']> = Omit<EventByType<K>, 'type'>;
type SimpleEventCallback<K extends AdapterEvent['type'] = AdapterEvent['type']> = (
  event: K extends 'data' ? ParsedDataEvent : EventByType<K>,
) => void;
type KeyedEventCallback<K extends string> = K extends AdapterEvent['type']
  ? K extends 'data'
    ? (event: Omit<ParsedDataEvent, 'type'>) => void
    : (event: KeyedAdapterEvent<K>) => void
  : never;
type EventCallbacks<Keys extends AdapterEvent['type'] | 'all' = AdapterEvent['type'] | 'all'> = {
  [K in Keys]: (K extends 'all' ? SimpleEventCallback : KeyedEventCallback<K>)[];
};

type GatewayOptions = { logLevel?: LogLevel };
class BridgethingGateway {
  private readonly logger: Logger;
  private readonly callbacks: EventCallbacks = {
    connected: [],
    disconnected: [],
    data: [],
    all: [],
  };
  constructor(
    private readonly adapter: Adapter,
    private readonly options: GatewayOptions = { logLevel: LogLevel.Log },
  ) {
    this.logger = new Logger('Gateway', options.logLevel || LogLevel.Log);
    adapter.on(allEvent => void this.handleEvent(allEvent));
  }

  /**
   * blocks until the bluetooth adapter is ready.
   * @throws THIS WILL THROW IF BLUETOOTH PERMISSION IS DENIED
   */
  init = () => this.adapter.init();

  on<K extends Lowercase<AdapterEvent['type']>>(type: K, callback: KeyedEventCallback<K>): void;
  on(callback: SimpleEventCallback): void;
  on<K extends Lowercase<AdapterEvent['type']>>(...params: [K, KeyedEventCallback<K>] | [SimpleEventCallback]) {
    if (typeof params[0] === 'string' && typeof params[1] === 'function')
      // this is a safe typecast bc of the generic drilling
      (this.callbacks[params[0]] as KeyedEventCallback<K>[]).push(params[1]);
    else if (typeof params[0] === 'function') this.callbacks.all.push(params[0]);
    else throw new Error('invalid callback.');
  }

  scanOn = () => this.adapter.scanOn();
  scanOff = () => this.adapter.scanOff();

  /** @throws THIS WILL THROW IF THE DEVICE IS NOT KNOWN/CONNECTED  */
  disconnect = (id: string) => this.adapter.disconnect(id);

  /** @throws THIS WILL THROW IF THE DEVICE IS NOT KNOWN/CONNECTED OR IF SEND FAILS */
  send = (deviceId: string, message: GatewayToBridgeMsg) => this.adapter.send(deviceId, encode(stripUuid(message)));

  private handleEvent(allEvent: AllAdapterEvent) {
    this.logger.trace('new event: ', allEvent);

    let data;
    if (allEvent.type === 'data' || allEvent.type === 'Data') data = decode(allEvent.data);
    const event = { ...allEvent, type: allEvent.type.toLowerCase(), data } as ParsedAdapterEvent;
    this.logger.trace('decoded event: ', event);

    this.callbacks.all.map(callback => callback(event as never));

    const { type: _, ...taggedEvent } = event;
    if (Array.isArray(this.callbacks[event.type]))
      this.callbacks[event.type].map(callback => callback(taggedEvent as never));
  }
}

const stripUuid = (message: GatewayToBridgeMsg) => ({ ...message, id: message.id.replaceAll('-', '') });

import { version } from './version';
const GATEWAY_VERSION = `v${version}`;

export {
  BridgethingGateway,
  GATEWAY_VERSION,
  type Adapter,
  type AdapterCallback,
  type AdapterEvent,
  type KeyedAdapterEvent,
  type KeyedEventCallback,
  type SimpleEventCallback,
};
