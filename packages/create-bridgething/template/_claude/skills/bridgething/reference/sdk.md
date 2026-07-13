# The webapp SDK (`@bridgething/client`)

A webapp reaches the daemon only through this one client: a typed facade over a
local WebSocket. You never open sockets or build wire messages yourself.

## Contents

- Read the types (the `.d.ts` files are the source of truth)
- Connect
- The three call shapes (events / requests / commands)
- Cookbook (player, asset, store, config, net, library)
- Full surface index (all 18)
- Gotchas

## Read the types

You have no editor to hover in, so read the shipped declarations directly. Every
method, event, and field carries a doc comment.

- `node_modules/@bridgething/client/dist/dispatch.generated.d.ts` - every
  `client.<surface>` method and event, with argument and return types.
- `node_modules/@bridgething/lib/dist/bindings/*.d.ts` - the payload and reply
  types (`PlayerState`, `MediaItem`, `NowPlayingUpdate`, `AssetGot`, ...) with
  per-field docs. `client.d.ts` is the webapp surface; `shared.ts` holds the
  shared data types.

Useful greps:

```bash
grep -n 'class PlayerSurface' -A40 node_modules/@bridgething/client/dist/dispatch.generated.d.ts
grep -n 'export type PlayerState ' -A12 node_modules/@bridgething/lib/dist/bindings/shared.ts
```

The `.d.ts` is the source of truth; this file is the map.

## Connect

```ts
import { BridgethingClient } from '@bridgething/client';

const client = new BridgethingClient();          // on device: talks to the local daemon
// dev against a real device from your laptop:
const client = new BridgethingClient({ url: import.meta.env.VITE_BRIDGETHING_URL });
```

Construct once and reuse (a `useMemo` in React). It auto-connects and
auto-reconnects. Watch the link with `client.on(e => ...)` (`e.type` is
`open` / `close` / `connecting` / `message`) and read `client.connectionState`
(`'connecting' | 'open' | 'closing' | 'closed'`). The daemon is always the sole
peer; you never address anything.

## The three call shapes

Every surface method is one of three shapes. Learn these and the whole SDK reads
the same way.

**1. Events** - the daemon pushes; you subscribe. `onXxx` returns an unsubscribe
function (call it in cleanup). Or `subscribe({...})` for several at once.

```ts
const off = client.player.onSnapshot(reply => setState(reply.state));
// ...later
off();
```

**2. Requests** - you ask, the daemon answers. Returns a tagged result; always
check `.ok`.

```ts
const res = await client.player.stateGet();
if (res.ok) console.log(res.response.state);
else console.warn(res.kind, res.error); // kind: 'domain' | 'protocol'
```

**3. Commands** - fire-and-forget. Returns `Promise<void>`; it resolves when the
daemon has taken the message, not when the phone finished acting.

```ts
await client.player.skipNext();
```

## Cookbook

**Now playing + transport (`client.player`)**

```ts
client.player.onSnapshot(r => setState(r.state)); // full PlayerState on every material change
client.player.stateGet().then(r => r.ok && setState(r.response.state)); // prime on mount

// PlayerState: { track?: MediaItem, playback: Playback, queue, options, context? }
// track.title / track.artist / track.album / track.artworkId / track.durationMs
// playback.state: 'stopped' | 'paused' | 'playing'; playback.positionMs; playback.shuffle

client.player.play({ uri: 'spotify:track:...' });
client.player.pause(); client.player.resume(); client.player.skipNext();
client.player.skipPrev({ allowSeeking: true });   // true = restart if progressed
client.player.seekTo({ positionMs: 30_000 });
client.player.setShuffle({ on: true });
client.player.setRepeat({ mode: 'all' });          // 'off' | 'all' | 'one'
```

Smooth progress: `onSnapshot` fires on material changes (track change, play/pause,
seek), NOT every second. Extrapolate the playhead locally from `playback.state`
and `positionMs` between snapshots.

**Artwork and images (`client.asset`)**

Player state gives opaque asset ids, never URLs. Fetch bytes and wrap a blob:

```ts
const res = await client.asset.get({ id: track.artworkId, requestId: crypto.randomUUID() });
if (res.ok) {
  const bytes = new Uint8Array(res.response.bytes as unknown as number[]);
  const url = URL.createObjectURL(new Blob([bytes], { type: res.response.mime ?? 'image/jpeg' }));
  // set <img src={url}>, and URL.revokeObjectURL(url) on cleanup
}
```

**Persistence (`client.store`)** - per-webapp key/value that survives restarts:

```ts
await client.store.put({ key: 'theme', value: 'dark' });
const r = await client.store.get({ key: 'theme' });   // r.ok && r.response.value
await client.store.delete({ key: 'theme' });
```

**User settings (`client.config`)** - values you declare in `manifest.json`; the
user edits them in the companion app, your side is read-only:

```ts
client.config.onChanged(c => applySetting(c.key, c.value));
const r = await client.config.get({ key: 'units' });
const all = await client.config.list();
```

The user edits these in the companion phone app. The scaffold ships a
companion-side settings page under `settings/` that reads and writes them via a
separate SDK, `@bridgething/client/settings` (a `postMessage` bridge to the
companion host, not this WebSocket). `manifest.json`'s `settings` field points
the companion at the built `dist/settings.html`.

NETWORK CAVEAT for the settings page: it runs on the phone with real internet,
but it loads from a `file://` origin and the webview enforces CORS on
fetch/XHR. WebSocket APIs work (the WS handshake is not CORS-gated); plain HTTP
APIs work only when the server sends permissive CORS headers (requests arrive
with `Origin: null`). Prefer the service's websocket API from the settings
page; for a CORS-strict HTTP-only service, fetch from the device webapp via
`client.net` (phone-tunneled, not origin-restricted) and keep the settings page
for typing and choosing.

**Internet through the phone (`client.net`)** - the device has no direct network;
`net.fetch` / websockets / a SOCKS proxy all tunnel through the phone. Needs the
matching `permissions` entry in `manifest.json` (`net.fetch`, `net.ws`,
`net.proxy`). Read the `NetSurface` block in the `.d.ts` for the request shapes.

**Library + search (`client.library`)** - `browse`, `search`, `recommendations`,
`favoritesList`/`favoritesContains` (requests), `favoritesToggle`/`favoritesSet`
(commands), `onFavoriteChanged` (event). All Spotify-backed.

## Full surface index

18 surfaces on `client.<name>`. Read each surface's class in
`dispatch.generated.d.ts` for its exact methods and types.

| Surface | What it does | Notable methods |
| --- | --- | --- |
| `player` | now-playing + transport | onSnapshot, stateGet, play, pause, resume, skipNext, skipPrev, seekTo, setShuffle, setRepeat, queueGet |
| `asset` | fetch blobs by opaque id | get, preload, onReady/onCleared |
| `store` | per-webapp key/value | get, put, delete |
| `config` | user settings from the manifest | get, list, onChanged |
| `capabilities` | which gateway features are live | get, onSnapshot |
| `library` | Spotify browse/search/favorites | browse, search, recommendations, favoritesList/Contains/Toggle/Set, onFavoriteChanged |
| `audio` | volume, mute, TTS, earcons | volumeUp/Down, setVolume, muteToggle, tts, earcon |
| `notifications` | phone notification actions | invokePositive/Negative, onPosted/Removed/Updated |
| `phone` | call control (Android; iOS uses iAP2) | stateGet, accept, end, initiate, mute, dtmf, onCallStarted/Updated/Ended |
| `geo` | location watches (phone-sourced) | watch, unwatch, getOnce, onPosition |
| `net` | HTTP / WS / SOCKS via the phone | fetch, wsOpen/wsSend/wsClose, streamOpen/streamCancel |
| `hardware` | backlight + ambient light sensor | displaySetMode, displaySetLevel, stateGet, onAmbientLightUpdate, onBrightnessChanged |
| `bluetooth` | adapter alias/bonds/discoverable | list, connect, forget, setAlias, enableDiscoverable |
| `system` | version, logs, power, diagnostics | versionRequest, logsTail/Subscribe, reboot, powerOff, factoryReset, onVersion, onLogEntry, onOta* |
| `time` | wall clock (device has no RTC) | get, onSnapshot, onChanged |
| `voice` | mic / push-to-talk (capability-gated) | pushToTalk, cancel, muteMic, stateGet |
| `webapp` | launcher: list/activate installed apps | list, current, activate, icon |
| `forward` | arbitrary passthrough to the companion | text, json, binary, onText/onJson/onBinary |

## Gotchas

- Always check `.ok` on request results; the daemon can answer with a domain or
  protocol error (e.g. `player.play` with a uri no gateway claims).
- Assume no phone: player, library, net all depend on the connected companion.
  Render an empty/placeholder state, do not throw.
- `config` is read-only from the webapp; only the companion writes it.
- Asset ids are opaque - never parse or construct them, never build image URLs.
- Some surfaces are capability-gated or platform-specific (e.g. `voice`, and
  `phone` on iOS). Check `client.capabilities` and handle absence.
