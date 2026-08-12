import { BRIDGETHING_PROFILE_UUID } from '@bridgething/lib';

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

export type SerialPort = {
  open(options: { baudRate: number }): Promise<void>;
  close(): Promise<void>;
  getInfo(): SerialPortInfo;
  readonly readable: ReadableStream<Uint8Array> | null;
  readonly writable: WritableStream<Uint8Array> | null;
  addEventListener(event: 'disconnect', listener: () => void): void;
  removeEventListener(event: 'disconnect', listener: () => void): void;
};

const SERIAL_BAUD_RATE = 9600;

export function serialAvailable(): boolean {
  return typeof navigator !== 'undefined' && 'serial' in navigator;
}

export async function requestSerialPort(serviceUuid: string = BRIDGETHING_PROFILE_UUID): Promise<SerialPort | null> {
  const serial = navigator.serial;
  if (!serial) {
    throw new Error(
      'navigator.serial is unavailable - Web Serial is Chromium-only (Chrome 117+ desktop, Chrome 138+ Android). Use the companion app on iOS Safari.',
    );
  }

  let port: SerialPort;
  try {
    port = await serial.requestPort({ allowedBluetoothServiceClassIds: [serviceUuid] });
  } catch (err) {
    if (err instanceof DOMException && err.name === 'NotFoundError') return null;
    throw err;
  }
  await port.open({ baudRate: SERIAL_BAUD_RATE });
  return port;
}

export async function permittedSerialPorts(serviceUuid: string = BRIDGETHING_PROFILE_UUID): Promise<SerialPort[]> {
  const serial = navigator.serial;
  if (!serial) return [];
  const ports = await serial.getPorts();
  return ports.filter(port => port.getInfo().bluetoothServiceClassId?.toLowerCase() === serviceUuid.toLowerCase());
}

export type SerialPump = {
  write(chunk: Uint8Array): Promise<void>;
  close(): Promise<void>;
};

export function pumpSerialPort(
  port: SerialPort,
  onBytes: (chunk: Uint8Array) => void,
  onClosed: () => void,
): SerialPump {
  const reader = port.readable?.getReader();
  const writer = port.writable?.getWriter();
  if (!reader || !writer) throw new Error('web-serial: port has no readable/writable stream');

  let done = false;
  const finish = () => {
    if (done) return;
    done = true;
    port.removeEventListener('disconnect', finish);
    onClosed();
  };
  port.addEventListener('disconnect', finish);

  const reading = (async () => {
    try {
      for (;;) {
        const { value, done: eof } = await reader.read();
        if (eof) break;
        if (value && value.byteLength > 0) onBytes(value);
      }
    } catch (err) {
      if (err instanceof Error && err.name !== 'AbortError') {
        console.error('[bridgething] web-serial read loop error', err);
      }
    } finally {
      finish();
    }
  })();

  return {
    write: chunk => writer.write(chunk),
    async close() {
      finish();
      await reader.cancel().catch(() => {});
      await writer.close().catch(() => {});
      await reading;
      await port.close().catch(() => {});
    },
  };
}
