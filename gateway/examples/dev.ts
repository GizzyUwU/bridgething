import { BridgethingGateway, type SimpleEventCallback } from '../src';

import { NodeAdapter } from '@bridgething/adapter-node';
import { LogLevel, type BridgeToGatewayMsg } from '@bridgething/lib';
import { randomUUIDv7, sleep } from 'bun';

const msgHandler: SimpleEventCallback = e => {
  console.log('>> js callback got new data!!', e);

  switch (e.type) {
    case 'connected':
      break;
    case 'disconnected':
      break;
    case 'message':
      return void handleMessage(e.deviceId, e.data);
  }
};

const handleMessage = async (deviceId: string, data: BridgeToGatewayMsg) => {
  switch (data.type) {
    case 'version':
      await gateway.send(deviceId, {
        id: randomUUIDv7(),
        meta: 'event',
        type: 'version',
        data: { version: 'v0.1.0-alpha1', app: 'development' },
      });

      // test send large file
      await gateway.send(deviceId, {
        id: randomUUIDv7(),
        meta: 'event',
        type: 'addFiles',
        data: {
          files: [
            {
              path: 'test',
              data: (() => {
                const randomData = new Uint8Array(1024 * 1024);
                crypto.getRandomValues(randomData);
                return randomData;
              })(),
            },
          ],
        },
      });

      break;
    case 'files':
      console.log(`files:`, data);
      break;
    case 'data':
      console.log(`data:`, data);
      break;
  }
};

const adapter = new NodeAdapter({
  adapterName: 'hci2',
  logLevelDirective: 'bridgething_adapter=trace,libbridgething=trace',
});
const gateway = new BridgethingGateway(adapter, { logLevel: LogLevel.Trace });
gateway.on(msgHandler);

await gateway.init();
await gateway.scanOn();

await sleep(1_000_000);
