/**
 * Sender-side flow control, ported from `host-gateway`'s `AckWindow`/`AckRegistry`
 * (`crates/host-gateway/src/transfer.rs`): the daemon acks the byte offset it has durably
 * consumed, and the streamer holds no more than `OTA_WINDOW_BYTES` past that. Acks are absolute
 * file offsets, so the baseline seeds to the resume point.
 *
 * The Rust version uses an atomic + `Notify` with `notify_one` specifically to avoid a lost
 * wakeup between a waiter's recheck and its `await`. That race doesn't exist here: JS is
 * single-threaded, so a synchronous "check the ack, then register a listener" pair can't be
 * interleaved by another callback - there's no gap for a `note()` to land in.
 */

const OTA_WINDOW_BYTES = 512 * 1024;
const OTA_ACK_TIMEOUT_MS = 30_000;

export type AckWindowOptions = {
  windowBytes?: number;
  ackTimeoutMs?: number;
};

export class AckWindow {
  private acked: number;
  private waiters: Array<() => void> = [];
  private readonly windowBytes: number;
  private readonly ackTimeoutMs: number;

  constructor(baseline: number, options: AckWindowOptions = {}) {
    this.acked = baseline;
    this.windowBytes = options.windowBytes ?? OTA_WINDOW_BYTES;
    this.ackTimeoutMs = options.ackTimeoutMs ?? OTA_ACK_TIMEOUT_MS;
  }

  note(received: number): void {
    if (received <= this.acked) return;
    this.acked = received;
    const waiters = this.waiters;
    this.waiters = [];
    for (const waiter of waiters) waiter();
  }

  get ackedBytes(): number {
    return this.acked;
  }

  async waitForRoom(offset: number): Promise<boolean> {
    for (;;) {
      if (offset < this.acked + this.windowBytes) return true;
      const priorAcked = this.acked;
      const progressed = await this.waitForProgress();
      if (!progressed && this.acked <= priorAcked) return false;
    }
  }

  private waitForProgress(): Promise<boolean> {
    return new Promise(resolve => {
      const timer = setTimeout(() => settle(false), this.ackTimeoutMs);
      const onProgress = () => settle(true);
      let settled = false;
      const settle = (result: boolean) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        const idx = this.waiters.indexOf(onProgress);
        if (idx >= 0) this.waiters.splice(idx, 1);
        resolve(result);
      };
      this.waiters.push(onProgress);
    });
  }
}

export class AckRegistry {
  private readonly windows = new Map<string, AckWindow>();

  constructor(private readonly windowOptions: AckWindowOptions = {}) {}

  register(transferId: string, baseline: number): AckWindow {
    const window = new AckWindow(baseline, this.windowOptions);
    this.windows.set(transferId, window);
    return window;
  }

  deregister(transferId: string): void {
    this.windows.delete(transferId);
  }

  note(transferId: string, received: number): void {
    this.windows.get(transferId)?.note(received);
  }
}

export class TransferStalledError extends Error {
  constructor(offset: number, totalSize: number) {
    super(`transfer stalled: no drain-ack progress at offset ${offset}/${totalSize}`);
    this.name = 'TransferStalledError';
  }
}
