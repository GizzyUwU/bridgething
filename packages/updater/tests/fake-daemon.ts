import type { Adapter, AdapterEvent, AdapterListener, BridgeToGatewayMsg } from '@bridgething/gateway';
import { Codec } from '@bridgething/lib';
import type { GatewayToBridgeMsg } from '@bridgething/lib/gateway';

export class FakeDaemon implements Adapter {
  private readonly listeners: Set<AdapterListener> = new Set();
  private readonly codec = new Codec();
  readonly sent: GatewayToBridgeMsg[] = [];

  on(listener: AdapterListener): void {
    this.listeners.add(listener);
  }
  off(listener: AdapterListener): void {
    this.listeners.delete(listener);
  }
  start(): Promise<void> {
    return Promise.resolve();
  }
  stop(): Promise<void> {
    return Promise.resolve();
  }
  disconnect(): Promise<void> {
    return Promise.resolve();
  }
  send(_deviceId: string, frame: Uint8Array): Promise<void> {
    this.sent.push(this.codec.decode<GatewayToBridgeMsg>(frame));
    return Promise.resolve();
  }

  connect(deviceId: string): void {
    this.emit({ type: 'connected', device: { id: deviceId, name: 'fake-device' } });
  }

  sendToDriver(deviceId: string, msg: BridgeToGatewayMsg): void {
    this.emit({ type: 'bytes', deviceId, data: this.codec.encode(msg) });
  }

  private emit(event: AdapterEvent): void {
    for (const listener of this.listeners) listener(event);
  }

  async waitForNext(predicate: (msg: GatewayToBridgeMsg) => boolean, timeoutMs = 2_000): Promise<GatewayToBridgeMsg> {
    const deadline = Date.now() + timeoutMs;
    let index = this.consumed;
    for (;;) {
      while (index < this.sent.length) {
        const msg = this.sent[index];
        index += 1;
        if (predicate(msg)) {
          this.consumed = index;
          return msg;
        }
      }
      if (Date.now() > deadline) {
        throw new Error('timed out waiting for outbound frame matching predicate');
      }
      await delay(5);
    }
  }

  private consumed = 0;
}

function delay(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}
