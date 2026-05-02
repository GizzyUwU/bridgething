import { BridgethingClient, type ConnectionState } from '@bridgething/client';
import { useEffect, useMemo, useState } from 'react';

// Production: webapp loads from the daemon on port 8891 of the device,
// so a relative `ws://<location.host>/` works. In dev (vite serving from
// your laptop), point at the device explicitly via env.
const wsUrl =
  import.meta.env.VITE_BRIDGETHING_URL ??
  (typeof window !== 'undefined' ? `ws://${window.location.host}/` : 'ws://127.0.0.1:8891/');

type Track = {
  name: string;
  artist: { name: string };
  album: { name: string };
  imageId: string;
};

type PlayerSnapshot = {
  isPaused: boolean;
  positionMs: number;
  track: Track;
};

export default function App() {
  const client = useMemo(() => new BridgethingClient({ url: wsUrl }), []);
  const [conn, setConn] = useState<ConnectionState>(client.connectionState);
  const [player, setPlayer] = useState<PlayerSnapshot | null>(null);
  const [artUrl, setArtUrl] = useState<string | null>(null);

  useEffect(() => {
    const offConn = client.on(event => {
      if (event.type === 'open' || event.type === 'close' || event.type === 'connecting') {
        setConn(client.connectionState);
      }
    });
    const offPlayer = client.player.onPlayerState(state => {
      setPlayer({
        isPaused: state.isPaused,
        positionMs: state.playbackPosition,
        track: state.track as unknown as Track,
      });
    });
    const offIdle = client.player.onPlayerIdle(() => setPlayer(null));
    return () => {
      offConn();
      offPlayer();
      offIdle();
    };
  }, [client]);

  // Resolve artwork via the asset request flow whenever the track changes.
  useEffect(() => {
    if (!player) {
      setArtUrl(null);
      return;
    }
    const id = player.track.imageId;
    if (!id) return;
    let revoked = false;
    let blobUrl: string | null = null;
    (async () => {
      const result = await client.asset.get({ id, requestId: crypto.randomUUID() });
      if (revoked) return;
      if (result.ok) {
        const blob = new Blob([result.response.bytes], { type: result.response.mime ?? 'image/jpeg' });
        blobUrl = URL.createObjectURL(blob);
        setArtUrl(blobUrl);
      }
    })();
    return () => {
      revoked = true;
      if (blobUrl) URL.revokeObjectURL(blobUrl);
    };
  }, [client, player?.track.imageId]);

  return (
    <div className="flex h-full w-full items-center justify-center gap-12 p-12">
      {artUrl ? (
        <img src={artUrl} alt="" className="h-full max-h-96 w-auto rounded-2xl shadow-2xl" />
      ) : (
        <div className="grid h-96 w-96 place-items-center rounded-2xl bg-neutral-900 text-neutral-600">
          {conn === 'open' ? 'no track' : conn}
        </div>
      )}
      <div className="flex flex-col gap-3">
        <div className="text-xs uppercase tracking-widest text-neutral-500">{conn}</div>
        {player ? (
          <>
            <div className="text-3xl font-semibold leading-tight">{player.track.name}</div>
            <div className="text-xl text-neutral-400">{player.track.artist.name}</div>
            <div className="text-sm text-neutral-600">{player.track.album.name}</div>
            <div className="mt-6 flex gap-4">
              <button
                className="rounded-full bg-white/10 px-6 py-3 text-sm font-medium transition active:bg-white/20"
                onClick={() => client.interaction.skipPrev()}>
                ◀◀
              </button>
              <button
                className="rounded-full bg-white px-6 py-3 text-sm font-medium text-black transition active:bg-neutral-300"
                onClick={() => (player.isPaused ? client.interaction.resume() : client.interaction.pause())}>
                {player.isPaused ? '▶' : '❚❚'}
              </button>
              <button
                className="rounded-full bg-white/10 px-6 py-3 text-sm font-medium transition active:bg-white/20"
                onClick={() => client.interaction.skipNext()}>
                ▶▶
              </button>
            </div>
          </>
        ) : (
          <div className="text-2xl text-neutral-500">connect a phone to see now playing</div>
        )}
      </div>
    </div>
  );
}
