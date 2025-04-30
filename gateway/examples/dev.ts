import { BridgethingGateway, type SimpleEventCallback } from '../src';

import { NodeAdapter } from '@bridgething/adapter-node';
import {
  BRIDGETHING_FILE_PORT,
  LIB_VERSION,
  LIBBRIDGETHING_VERSION,
  LogLevel,
  type BridgeToGatewayMsg,
  type GatewayMeta,
} from '@bridgething/lib';
import { randomUUIDv7, sleep } from 'bun';

type NextTask = 'navigate';
const WAIT_FOR = new Map<string, NextTask>();

const msgHandler: SimpleEventCallback = e => {
  // console.log('>> js callback got new data!!', e);

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
    event: 'add',
    data: {
      files: [
        { path: 'index.html', data: html },
        { path: 'index.js', data: indexJs },
        { path: 'ui.js', data: uiJs },
        { path: 'websocket.js', data: websocketJs },
      ],
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
        data: makeGatewayMeta(),
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
      switch (msg.encoding) {
        case 'text':
          console.log('>> got forwarded text data:', msg.data);
          await gateway.send(deviceId, {
            id: randomUUIDv7(),
            meta: 'event',
            type: 'forward',
            encoding: 'text',
            data: 'hello from the gateway!',
          });
          break;
        case 'json':
          console.log('>> got forwarded json data:', msg.data);
          await gateway.send(deviceId, {
            id: randomUUIDv7(),
            meta: 'event',
            type: 'forward',
            encoding: 'json',
            data: { message: 'hello from the gateway!' },
          });
          break;
        case 'binary':
          console.log('>> got forwarded binary data:', msg.data);
          await gateway.send(deviceId, {
            id: randomUUIDv7(),
            meta: 'event',
            type: 'forward',
            encoding: 'binary',
            data: bufferToBase64(new Uint8Array([69, 69, 69, 69])),
          });
          break;
      }

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
            event: 'navigate',
            data: { url: `http://localhost:${BRIDGETHING_FILE_PORT}/index.html` },
          });
        }
      }
      break;
    }
  }
};

const bufferToBase64 = (buffer: Uint8Array) => Buffer.from(buffer).toString('base64');
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
const gateway = new BridgethingGateway(adapter, { logLevel: LogLevel.Trace });
gateway.on(msgHandler);

await gateway.init();
await gateway.scanOn();

await sleep(1_000_000);
