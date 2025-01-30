import { BridgethingGateway, type SimpleEventCallback } from '../src';

import { NodeAdapter } from '@bridgething/adapter-node';
import { LogLevel, type BridgeToGatewayMsg } from '@bridgething/lib';
import { sleep } from 'bun';

const msgHandler: SimpleEventCallback = e => {
  console.log('>> js callback got new data!!', e);

  switch (e.type) {
    case 'connected':
      break;
    case 'disconnected':
      break;
    case 'data':
      return void handleData(e.deviceId, e.data);
  }
};

const handleData = async (deviceId: string, data: BridgeToGatewayMsg) => {
  switch (data.type) {
    case 'version':
      await gateway.send(deviceId, { id: data.id, type: 'version', gateway: 'v0.1.0-alpha1', app: 'development' });
      break;
    case 'request':
      break;
    case 'response':
      break;
  }
};

const gateway = new BridgethingGateway(new NodeAdapter('bridgething_adapter=trace'), { logLevel: LogLevel.Trace });
gateway.on(msgHandler);

await gateway.init('hci1');
await gateway.scanOn();

await sleep(1_000_000);
