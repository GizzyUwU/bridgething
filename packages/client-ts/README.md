# @bridgething/client

The SDK a [bridgething](https://github.com/JoeyEamigh/bridgething) webapp uses to
talk to the on-device daemon on a Spotify Car Thing. It is a typed facade over
the daemon's local WebSocket.

```ts
import { BridgethingClient } from '@bridgething/client';

const client = new BridgethingClient(); // auto-connects to the daemon, auto-reconnects
client.player.onSnapshot(r => render(r.state));
client.player.skipNext();
```

Every surface is fully typed with doc comments in
`dist/dispatch.generated.d.ts` or hover in your editor.

## Getting started

Scaffold a new webapp (React + Vite + Tailwind, this client preinstalled):

```sh
bun create bridgething my-app
```

- Full docs: <https://bridgething.com/docs>
- Source: <https://github.com/JoeyEamigh/bridgething>
