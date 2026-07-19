import { BRIDGETHING_PROFILE_UUID } from '@bridgething/lib';

import type { Adapter, AdapterEvent, AdapterListener, Device } from './index.js';

/**
 * Web Serial adapter for the bridgething wire protocol.
 *
 * Chrome (desktop M117+, Android M138+) extended `navigator.serial` to enumerate RFCOMM SPP
 * services on already-paired Bluetooth Classic devices by `bluetoothServiceClassId`. We filter
 * on `BRIDGETHING_PROFILE_UUID` so the chooser only shows actual bridgething peers.
 *
 * Pre-pairing required: the user pairs the Car Thing once via OS Bluetooth settings; subsequent
 * page loads get the already-permitted device from `getPorts()` without re-prompting.
 *
 * iOS Safari and Firefox have no Web Serial; iOS consumers must use the companion app's iAP2 path.
 */

declare global {
  interface Navigator {
    readonly serial?: SerialAPI;
  }
}

type SerialAPI = {
  requestPort(options?: SerialPortRequestOptions): Promise<SerialPort>;
  getPorts(): Promise<SerialPort[]>;
};

type SerialPortRequestOptions = {
  filters?: SerialPortFilter[];
  allowedBluetoothServiceClassIds?: string[];
};

type SerialPortFilter = {
  bluetoothServiceClassId?: string;
};

type SerialPortInfo = {
  bluetoothServiceClassId?: string;
  usbVendorId?: number;
  usbProductId?: number;
};

type SerialPort = {
  open(options: { baudRate: number }): Promise<void>;
  close(): Promise<void>;
  getInfo(): SerialPortInfo;
  readonly readable: ReadableStream<Uint8Array> | null;
  readonly writable: WritableStream<Uint8Array> | null;
  addEventListener(event: 'disconnect', listener: () => void): void;
  removeEventListener(event: 'disconnect', listener: () => void): void;
};

type SessionState = {
  port: SerialPort;
  reader: ReadableStreamDefaultReader<Uint8Array> | null;
  writer: WritableStreamDefaultWriter<Uint8Array> | null;
  readLoop: Promise<void> | null;
  disconnectListener: () => void;
};

// baudRate is required by the Web Serial spec but ignored by BR/EDR SPP.
const SERIAL_BAUD_RATE = 9600;

export type WebSerialAdapterOptions = {
  /** Override the SPP service UUID filter. Defaults to the canonical
   * bridgething profile UUID. */
  serviceUuid?: string;
};

export class WebSerialAdapter implements Adapter {
  private readonly listeners: Set<AdapterListener> = new Set();
  private readonly sessions: Map<string, SessionState> = new Map();
  private readonly serviceUuid: string;
  private running = false;

  constructor(options: WebSerialAdapterOptions = {}) {
    this.serviceUuid = options.serviceUuid ?? BRIDGETHING_PROFILE_UUID;
  }

  on(listener: AdapterListener): void {
    this.listeners.add(listener);
  }

  off(listener: AdapterListener): void {
    this.listeners.delete(listener);
  }

  async start(): Promise<void> {
    if (this.running) return;
    if (!navigator.serial) {
      throw new Error(
        'navigator.serial is unavailable - Web Serial is Chromium-only (Chrome 117+ desktop, Chrome 138+ Android). Use the companion app on iOS Safari.',
      );
    }
    this.running = true;

    // getPorts() doesn't need a user gesture; reconnects already-permitted devices silently
    const existing = await navigator.serial.getPorts();
    for (const port of existing) {
      if (this.isBridgethingPort(port)) {
        await this.openSession(port);
      }
    }
  }

  async stop(): Promise<void> {
    if (!this.running) return;
    this.running = false;
    const sessions = Array.from(this.sessions.values());
    this.sessions.clear();
    await Promise.all(sessions.map(s => closeSession(s)));
  }

  async disconnect(deviceId: string): Promise<void> {
    const session = this.sessions.get(deviceId);
    if (!session) return;
    this.sessions.delete(deviceId);
    await closeSession(session);
    this.emit({ type: 'disconnected', deviceId });
  }

  async send(deviceId: string, frame: Uint8Array): Promise<void> {
    const session = this.sessions.get(deviceId);
    if (!session || !session.writer) {
      throw new Error(`web-serial: no active session for ${deviceId}`);
    }
    await session.writer.write(frame);
  }

  /**
   * Prompt the user to pick a bridgething device. Must be called from a
   * user gesture (button click, etc); browsers refuse to show the picker
   * otherwise.
   */
  async requestDevice(): Promise<Device | null> {
    if (!navigator.serial) {
      throw new Error('navigator.serial is unavailable');
    }
    let port: SerialPort;
    try {
      port = await navigator.serial.requestPort({
        allowedBluetoothServiceClassIds: [this.serviceUuid],
      });
    } catch (err) {
      // user cancelled the chooser
      if (err instanceof DOMException && err.name === 'NotFoundError') return null;
      throw err;
    }
    return this.openSession(port);
  }

  private isBridgethingPort(port: SerialPort): boolean {
    const info = port.getInfo();
    return info.bluetoothServiceClassId?.toLowerCase() === this.serviceUuid.toLowerCase();
  }

  private async openSession(port: SerialPort): Promise<Device> {
    await port.open({ baudRate: SERIAL_BAUD_RATE });
    const info = port.getInfo();
    // SerialPortInfo has no stable device identifier; the counter suffix disambiguates within a page load
    const deviceId = `web-serial:${info.bluetoothServiceClassId ?? 'unknown'}:${this.sessions.size}`;
    const device: Device = { id: deviceId, name: 'Bridgething' };

    const reader = port.readable?.getReader();
    const writer = port.writable?.getWriter();
    if (!reader || !writer) {
      await port.close();
      throw new Error('web-serial: port has no readable/writable stream');
    }

    const disconnectListener = () => {
      this.handleDisconnect(deviceId);
    };
    port.addEventListener('disconnect', disconnectListener);

    const session: SessionState = {
      port,
      reader,
      writer,
      readLoop: null,
      disconnectListener,
    };
    this.sessions.set(deviceId, session);

    session.readLoop = this.runReadLoop(deviceId, reader);

    this.emit({ type: 'connected', device });
    return device;
  }

  private async runReadLoop(deviceId: string, reader: ReadableStreamDefaultReader<Uint8Array>): Promise<void> {
    try {
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        if (value && value.byteLength > 0) {
          this.emit({ type: 'bytes', deviceId, data: value });
        }
      }
    } catch (err) {
      if (err instanceof Error && err.name !== 'AbortError') {
        console.error('[bridgething] web-serial read loop error', err);
      }
    } finally {
      this.handleDisconnect(deviceId);
    }
  }

  private handleDisconnect(deviceId: string): void {
    const session = this.sessions.get(deviceId);
    if (!session) return;
    this.sessions.delete(deviceId);
    void closeSession(session);
    this.emit({ type: 'disconnected', deviceId });
  }

  private emit(event: AdapterEvent): void {
    for (const listener of this.listeners) {
      try {
        listener(event);
      } catch (err) {
        console.error('[bridgething] web-serial listener threw', err);
      }
    }
  }
}

async function closeSession(session: SessionState): Promise<void> {
  session.port.removeEventListener('disconnect', session.disconnectListener);
  try {
    await session.reader?.cancel();
  } catch {
    // already cancelled; ignore
  }
  try {
    session.reader?.releaseLock();
  } catch {
    // reader already released
  }
  try {
    await session.writer?.close();
  } catch {
    // writer already closed
  }
  try {
    session.writer?.releaseLock();
  } catch {
    // writer already released
  }
  if (session.readLoop) {
    try {
      await session.readLoop;
    } catch {
      // read loop already cleaned up
    }
  }
  try {
    await session.port.close();
  } catch {
    // port already closed
  }
}
