type BLEAdapter = {
  on(callback: (event: string) => void): void;

  init(): Promise<void>;
  init(adapterName?: string | null): Promise<void>;

  scanOn(): void;
  scanOff(): void;

  disconnect(macAddress: string): void;

  send(): Promise<void>;
};

export { type BLEAdapter };
