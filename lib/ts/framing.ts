import { CodecError, FRAME_HEADER_LENGTH, parseFrameHeader } from './codec';

/**
 * Per-stream buffer that takes raw byte chunks and yields complete frames.
 *
 * Bytes from a stream-oriented transport (RFCOMM, EASession, sockets) arrive
 * without respect to the bridgething frame boundary. `FrameAccumulator` keeps
 * a rolling buffer, validates each header as soon as 16 bytes are available,
 * waits for the full payload, and pops one complete frame at a time.
 *
 * Caller-driven: feed bytes with `append`, drain with repeated `nextFrame()`
 * until it returns null. Not thread-safe.
 */
export class FrameTooLargeError extends CodecError {
  constructor(
    public readonly payloadLength: number,
    public readonly maxPayloadSize: number,
  ) {
    super(`frame payload ${payloadLength} exceeds max ${maxPayloadSize}`, 'payload-too-short');
    this.name = 'FrameTooLargeError';
  }
}

const DEFAULT_MAX_PAYLOAD_SIZE = 8 * 1024 * 1024;

export class FrameAccumulator {
  private buffer: Uint8Array = new Uint8Array(0);

  constructor(public readonly maxPayloadSize: number = DEFAULT_MAX_PAYLOAD_SIZE) {}

  append(chunk: Uint8Array): void {
    if (chunk.length === 0) return;
    const merged = new Uint8Array(this.buffer.length + chunk.length);
    merged.set(this.buffer, 0);
    merged.set(chunk, this.buffer.length);
    this.buffer = merged;
  }

  /**
   * Pops one complete frame from the head of the buffer if available.
   * Returns null when the buffer doesn't yet contain a full header + payload.
   * Throws on bad magic, unsupported header bytes, or oversized payloads —
   * the caller is expected to drop the connection in those cases since the
   * stream has lost framing and there's no safe resync.
   */
  nextFrame(): Uint8Array | null {
    if (this.buffer.length < FRAME_HEADER_LENGTH) return null;
    const header = parseFrameHeader(this.buffer);
    if (header.payloadLength > this.maxPayloadSize) {
      throw new FrameTooLargeError(header.payloadLength, this.maxPayloadSize);
    }
    const total = FRAME_HEADER_LENGTH + header.payloadLength;
    if (this.buffer.length < total) return null;
    const frame = this.buffer.subarray(0, total);
    this.buffer = this.buffer.subarray(total);
    // Detach view so the caller can hold onto `frame` without the underlying
    // buffer being mutated by future appends. (subarray shares storage.)
    const owned = new Uint8Array(frame.length);
    owned.set(frame);
    return owned;
  }

  get bufferedByteCount(): number {
    return this.buffer.length;
  }

  reset(): void {
    this.buffer = new Uint8Array(0);
  }
}
