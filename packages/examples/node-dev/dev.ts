import { NodeAdapter } from '@bridgething/adapter-node';
import { BRIDGETHING_FILE_PORT, LIB_VERSION, LIBBRIDGETHING_VERSION, LogVerbosity } from '@bridgething/lib';
import type { BridgeToGatewayMsg, GatewayMeta } from '@bridgething/lib/gateway';
import { newUuid } from '@bridgething/lib/uuid';
import { sleep } from 'bun';

import { BridgethingGateway, type GatewayEvent } from '../src';

const handleMessage = async (deviceId: string, msg: BridgeToGatewayMsg) => {
  switch (msg.data.type) {
    case 'version':
      await gateway.send(deviceId, {
        id: newUuid(),
        meta: { kind: 'event' },
        data: { type: 'version', data: makeGatewayMeta() },
      });

      await pushAssets(deviceId);
      await sendNavigate(deviceId);
      break;

    case 'file':
      console.log('file event:', msg.data.data);
      break;

    case 'forward': {
      const forward = msg.data.data;
      switch (forward.encoding) {
        case 'text':
          console.log('>> got forwarded text:', forward.data);
          await gateway.send(deviceId, {
            id: newUuid(),
            meta: { kind: 'event' },
            data: {
              type: 'forward',
              data: { encoding: 'text', data: 'hello from the gateway!' },
            },
          });
          break;
        case 'json':
          console.log('>> got forwarded json:', forward.data);
          await gateway.send(deviceId, {
            id: newUuid(),
            meta: { kind: 'event' },
            data: {
              type: 'forward',
              data: { encoding: 'json', data: { message: 'hello from the gateway!' } },
            },
          });
          break;
        case 'binary':
          console.log('>> got forwarded binary:', forward.data);
          await gateway.send(deviceId, {
            id: newUuid(),
            meta: { kind: 'event' },
            data: {
              type: 'forward',
              data: { encoding: 'binary', data: new Uint8Array([69, 69, 69, 69]) },
            },
          });
          break;
      }
      break;
    }

    case 'ack':
    case 'done':
      // Bare ack/done events with `meta.kind: 'event'` (rather than a
      // response) are unusual - the daemon almost always wraps them in a
      // response so the gateway's request() can resolve. Log and ignore.
      console.log('unexpected ack/done as event:', msg);
      break;
  }
};

const pushAssets = async (deviceId: string) => {
  console.log('>> pushing html + js assets');
  const [html, indexJs, uiJs, websocketJs] = await Promise.all([
    Bun.file(import.meta.dir + '/assets/index.html').bytes(),
    Bun.file(import.meta.dir + '/assets/index.js').bytes(),
    Bun.file(import.meta.dir + '/assets/ui.js').bytes(),
    Bun.file(import.meta.dir + '/assets/websocket.js').bytes(),
  ]);

  const response = await gateway.request(deviceId, {
    type: 'file',
    data: {
      event: 'add',
      data: {
        files: [
          { path: 'index.html', data: html },
          { path: 'index.js', data: indexJs },
          { path: 'ui.js', data: uiJs },
          { path: 'websocket.js', data: websocketJs },
        ],
      },
    },
  });

  if (response.data.type !== 'ack' && response.data.type !== 'done') {
    console.warn('asset push got unexpected response data:', response.data);
  }
};

const sendNavigate = async (deviceId: string) => {
  console.log('>> navigating to index.html');
  await gateway.send(deviceId, {
    id: newUuid(),
    meta: { kind: 'command' },
    data: {
      type: 'chrome',
      data: {
        event: 'navigate',
        data: { url: `http://localhost:${BRIDGETHING_FILE_PORT}/index.html` },
      },
    },
  });
};

const handleEvent = (event: GatewayEvent) => {
  switch (event.type) {
    case 'connected':
      console.log('++ connected:', event.device);
      break;
    case 'disconnected':
      console.log('-- disconnected:', event.deviceId);
      break;
    case 'message':
      void handleMessage(event.deviceId, event.message);
      break;
    case 'decodeError':
      console.error('!! decode error on', event.deviceId, event.description);
      break;
  }
};

const makeGatewayMeta = (): GatewayMeta => ({
  adapterVersion: 'v0.1.0-alpha1',
  appVersion: 'v0.1.0-alpha1',
  appName: 'development',
  libbridgethingVersion: LIBBRIDGETHING_VERSION,
  libVersion: LIB_VERSION,
  osName: 'development',
});

const adapter = new NodeAdapter({
  adapterName: 'hci2',
  logLevelDirective: 'bridgething_adapter=trace,libbridgething=trace',
});
const gateway = new BridgethingGateway(adapter, { logLevel: LogVerbosity.Trace });
gateway.on(handleEvent);

await gateway.start();

await sleep(1_000_000);
