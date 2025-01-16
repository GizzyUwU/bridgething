import { scanAndConnect } from '@bridgething/adapter';
import { type BLEAdapter } from '@bridgething/gateway';

class DevGateway implements BLEAdapter {
  constructor() {}

  async scan() {
    await scanAndConnect();
  }
}

const gateway = new DevGateway();
await gateway.scan();
