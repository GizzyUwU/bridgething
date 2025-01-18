import { PlugAdapter } from '@bridgething/adapter';
import { sleep } from 'bun';

const adapter = new PlugAdapter();
adapter.on(e => console.log('>> js got new data!!', e));

await adapter.init('hci1');

await sleep(1_000_000);
