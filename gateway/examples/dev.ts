import { BridgethingGateway } from '../src';

import { PlugAdapter } from '@bridgething/adapter';
import { sleep } from 'bun';

const gateway = new BridgethingGateway(new PlugAdapter());
gateway.on(e => console.log('>> js callback got new data!!', e));

await gateway.init('hci1');

await sleep(1_000_000);
