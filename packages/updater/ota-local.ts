import { BridgethingGateway } from '@bridgething/gateway';
import { OtaDriver } from './src/driver.js';
import { fileArtifactSource } from './src/node.js';
import { NetworkAdapter } from './src/websocket.js';
const swu = process.argv[2];
const gateway = new BridgethingGateway(new NetworkAdapter({ discovery: 'ws://10.42.1.114:8892/' }));
await gateway.start();
const { deviceId, meta } = await new Promise<any>((res, rej) => {
  const t = setTimeout(() => {
    off();
    rej(new Error('no announce'));
  }, 30_000);
  const off = gateway.on(ev => {
    if (ev.type !== 'message' || ev.message.data.type !== 'version') return;
    clearTimeout(t);
    off();
    res({ deviceId: ev.deviceId, meta: ev.message.data.data });
  });
});
console.log(`device image=${meta.imageVersion} daemon=${meta.appVersion} (${meta.imageVariant}/${meta.channel})`);
const driver = new OtaDriver(gateway, deviceId);
let last = -1;
try {
  const snap = await driver.pushImage({
    source: await fileArtifactSource(swu),
    onProgress: s => {
      const p = 'percent' in s ? (s.percent as number) : -1;
      if (s.phase !== 'streaming' || p - last >= 20 || p === 100) {
        last = p;
        console.log(`  ${s.phase}${p >= 0 ? ' ' + p + '%' : ''}`);
      }
    },
  });
  console.log('final:', JSON.stringify(snap));
} finally {
  driver.close();
}
