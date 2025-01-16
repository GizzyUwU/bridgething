interface BLEAdapter {
  scan(): Promise<void>;
}

export { type BLEAdapter };
