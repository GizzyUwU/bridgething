import { BridgethingClient, type ConnectionState, type PlayerState } from '@bridgething/client';
import { useEffect, useMemo, useState } from 'react';

// Production: the webapp is served by the daemon on the device, so a
// relative `ws://<location.host>/` reaches it. In dev (vite on your
// laptop) point at the device explicitly with VITE_BRIDGETHING_URL.
const wsUrl =
  import.meta.env.VITE_BRIDGETHING_URL ??
  (typeof window !== 'undefined' ? `ws://${window.location.host}/` : 'ws://127.0.0.1:8891/');

export default function App() {
  const client = useMemo(() => new BridgethingClient({ url: wsUrl }), []);
  const [conn, setConn] = useState<ConnectionState>(client.connectionState);
  const [state, setState] = useState<PlayerState | null>(null);
  const [artUrl, setArtUrl] = useState<string | null>(null);

  useEffect(() => {
    const offConn = client.on(event => {
      if (event.type === 'open' || event.type === 'close' || event.type === 'connecting') {
        setConn(client.connectionState);
      }
    });
    const offSnapshot = client.player.onSnapshot(reply => setState(reply.state));
    client.player.stateGet().then(r => r.ok && setState(r.response.state));
    return () => {
      offConn();
      offSnapshot();
    };
  }, [client]);

  const track = state?.track ?? null;
  const artworkId = track?.artworkId ?? null;

  useEffect(() => {
    if (!artworkId) {
      setArtUrl(null);
      return;
    }
    let revoked = false;
    let blobUrl: string | null = null;
    (async () => {
      const result = await client.asset.get({ id: artworkId, requestId: crypto.randomUUID() });
      if (revoked) return;
      if (result.ok) {
        const bytes = new Uint8Array(result.response.bytes as unknown as number[]);
        blobUrl = URL.createObjectURL(new Blob([bytes], { type: result.response.mime ?? 'image/jpeg' }));
        setArtUrl(blobUrl);
      }
    })();
    return () => {
      revoked = true;
      if (blobUrl) URL.revokeObjectURL(blobUrl);
    };
  }, [client, artworkId]);

  const playing = state?.playback.state === 'playing';

  return (
    <div className="flex h-full w-full items-center justify-center gap-12 bg-bt-charcoal p-12 text-bt-off-white">
      {artUrl ? (
        <img src={artUrl} alt="" className="h-full max-h-96 w-auto rounded-2xl shadow-2xl" />
      ) : (
        <div className="grid h-96 w-96 place-items-center rounded-2xl bg-black/40 text-sm text-bt-soft-gray">
          {conn === 'open' ? 'no track' : conn}
        </div>
      )}
      <div className="flex flex-col gap-3">
        <div className="text-xs uppercase tracking-widest text-bt-soft-gray">{conn}</div>
        {track ? (
          <>
            <div className="text-3xl font-semibold leading-tight">{track.title ?? 'unknown'}</div>
            <div className="text-xl text-bt-soft-gray">{track.artist ?? ''}</div>
            <div className="text-sm text-bt-soft-gray/70">{track.album ?? ''}</div>
            <div className="mt-6 flex gap-4">
              <button
                className="rounded-full bg-white/10 px-6 py-3 text-sm font-medium transition active:bg-white/20"
                onClick={() => client.player.skipPrev()}>
                ◀◀
              </button>
              <button
                className="rounded-full bg-bt-blue px-6 py-3 text-sm font-medium text-bt-charcoal transition active:scale-95"
                onClick={() => (playing ? client.player.pause() : client.player.resume())}>
                {playing ? '❚❚' : '▶'}
              </button>
              <button
                className="rounded-full bg-white/10 px-6 py-3 text-sm font-medium transition active:bg-white/20"
                onClick={() => client.player.skipNext()}>
                ▶▶
              </button>
            </div>
          </>
        ) : (
          <div className="text-2xl text-bt-soft-gray">connect a phone to see now playing</div>
        )}
      </div>
    </div>
  );
}
