import type { BridgeToGatewayMsg, GatewayToBridgeMsg } from '@bridgething/lib';
import { Logger, LogLevel } from '@bridgething/lib';

type AdapterCallback = (event: AllAdapterEvent) => void;
type Adapter = {
  on(callback: AdapterCallback): void;

  init(): Promise<void>;

  scanOn(): Promise<void>;
  scanOff(): Promise<void>;

  disconnect(deviceId: string): Promise<void>;

  send(deviceId: string, message: GatewayToBridgeMsg): Promise<void> | void;
};

type AdapterEvent =
  | { type: 'connected'; name: string; deviceId: string }
  | { type: 'disconnected'; deviceId: string }
  | { type: 'message'; deviceId: string; data: BridgeToGatewayMsg };
type AllAdapterEvent = AdapterEvent extends infer T
  ? T extends { type: infer U extends string }
    ? Omit<T, 'type'> & { type: U | Capitalize<U> }
    : never
  : never;

type ParsedDataEvent = { type: 'data'; deviceId: string; data: BridgeToGatewayMsg };

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
    message: [],
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

  on<K extends AdapterEvent['type']>(type: K, callback: KeyedEventCallback<K>): void;
  on(callback: SimpleEventCallback): void;
  on<K extends AdapterEvent['type']>(...params: [K, KeyedEventCallback<K>] | [SimpleEventCallback]) {
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
  send = (deviceId: string, message: GatewayToBridgeMsg) => this.adapter.send(deviceId, message);

  private handleEvent(allEventData: AllAdapterEvent) {
    const event = lowercaseEvent(allEventData);
    this.logger.trace('new event: ', event);

    this.callbacks.all.map(callback => callback(event as never));

    const { type: _, ...taggedEvent } = event;
    if (Array.isArray(this.callbacks[event.type]))
      this.callbacks[event.type].map(callback => callback(taggedEvent as never));
  }
}

const lowercaseEvent = <T extends AllAdapterEvent>({ type, ...event }: T) =>
  ({ ...event, type: type.toLowerCase() }) as AdapterEvent;

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
