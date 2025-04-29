import { BridgethingGateway, type SimpleEventCallback } from '../src';

import { NodeAdapter } from '@bridgething/adapter-node';
import { BRIDGETHING_FILE_PORT, LogLevel, type BridgeToGatewayMsg } from '@bridgething/lib';
import { randomUUIDv7, sleep } from 'bun';

type NextTask = 'navigate';
const WAIT_FOR = new Map<string, NextTask>();

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

const appInit = async (deviceId: string) => {
  console.log('>> sending html file to device');
  const html = await Bun.file(import.meta.dir + '/assets/index.html').bytes();
  const indexJs = await Bun.file(import.meta.dir + '/assets/index.js').bytes();
  const uiJs = await Bun.file(import.meta.dir + '/assets/ui.js').bytes();
  const websocketJs = await Bun.file(import.meta.dir + '/assets/websocket.js').bytes();

  const id = randomUUIDv7();
  WAIT_FOR.set(id, 'navigate');

  await gateway.send(deviceId, {
    id,
    meta: 'event',
    type: 'file',
    data: {
      type: 'add',
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
};

const handleMessage = async (deviceId: string, msg: BridgeToGatewayMsg) => {
  switch (msg.type) {
    case 'version':
      await gateway.send(deviceId, {
        id: randomUUIDv7(),
        meta: 'event',
        type: 'version',
        data: { version: 'v0.1.0-alpha1', app: 'development' },
      });

      await appInit(deviceId);

      // test send large file
      // console.log('>> sending large test file to device');
      // await gateway.send(deviceId, {
      //   id: randomUUIDv7(),
      //   meta: 'event',
      //   type: 'file',
      //   data: {
      //     type: 'add',
      //     data: {
      //       files: [
      //         {
      //           path: 'test.bin',
      //           data: (() => {
      //             const randomData = new Uint8Array(1024 * 1024);
      //             crypto.getRandomValues(randomData);
      //             return randomData;
      //           })(),
      //         },
      //       ],
      //     },
      //   },
      // });

      break;
    case 'file':
      console.log(`file:`, msg);
      break;
    case 'forward':
      console.log(`forwarded data:`, msg);
      if (msg.data.contentType === 'text') console.log('>> got forwarded text data:', msg.data.content);
      else if (msg.data.contentType === 'binary') console.log('>> got forwarded binary data:', msg.data.contentType);
      await gateway.send(deviceId, {
        id: randomUUIDv7(),
        meta: 'forward',
        type: 'forward',
        data: {
          contentType: 'text',
          content: 'hello from the gateway!',
        },
      });

      break;
    case 'ack':
    case 'done': {
      console.log(`ack/done:`, msg);
      if (!('requestId' in msg)) return;

      if (WAIT_FOR.has(msg.requestId)) {
        const task = WAIT_FOR.get(msg.requestId);
        WAIT_FOR.delete(msg.requestId);
        console.log('>> waited for task completed', msg.requestId);

        if (task === 'navigate') {
          console.log('>> navigating to index.html');
          await gateway.send(deviceId, {
            id: randomUUIDv7(),
            meta: 'event',
            type: 'chrome',
            data: {
              type: 'navigate',
              data: { url: `http://localhost:${BRIDGETHING_FILE_PORT}/index.html` },
            },
          });
        }
      }
      break;
    }
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
