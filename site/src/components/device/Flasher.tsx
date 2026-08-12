import type { GestureReason } from 'flashthing-wasm';
import { useState } from 'preact/hooks';
import { loadBundle, prepareFlash, webusbSupported, type FlashEvent } from '../../lib/flasher';
import { ConsoleLog } from '../console/ConsoleLog';
import { useConsoleLog } from '../console/useConsoleLog';

export type Build = {
  slug: string;
  name: string;
  description: string;
  stability: string;
  default: boolean;
  version: string;
  url: string;
  size: number;
};

const LOG_EVERY_BYTES = 50 * 1024 * 1024;

function mb(bytes: number): string {
  return `${(bytes / 1024 / 1024).toFixed(1)} mb`;
}

function duration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 1000));
  const minutes = Math.floor(total / 60);
  return minutes ? `${minutes}m${String(total % 60).padStart(2, '0')}s` : `${total}s`;
}

export function Flasher({ builds }: { builds: Build[] }) {
  const { lines, say } = useConsoleLog();
  const [selected, setSelected] = useState(builds.find(b => b.default)?.slug ?? builds[0]?.slug ?? '');
  const [percent, setPercent] = useState(0);
  const [status, setStatus] = useState('idle');
  const [running, setRunning] = useState(false);
  const [gesture, setGesture] = useState<(() => void) | null>(null);

  if (!webusbSupported()) {
    return (
      <div class="border border-dashed border-white/25 p-6">
        <p class="m-0 font-mono text-sm text-white/70">
          this browser has no webusb. use chrome, edge, or another chromium browser, or flash with{' '}
          <code>flashthing-cli</code> instead.
        </p>
      </div>
    );
  }

  const onEvent = (event: FlashEvent) => {
    switch (event.type) {
      case 'findingDevice':
        say('looking for a device in burn mode');
        return;
      case 'deviceMode':
        say(`device mode: ${event.mode}`, event.mode === 'notFound' ? 'warn' : 'info');
        return;
      case 'connecting':
        say('connecting');
        return;
      case 'connected':
        say('connected', 'ok');
        return;
      case 'bl2Boot':
        say('booting bl2 into burn mode');
        return;
      case 'resetting':
        say('device resetting, waiting for it to come back');
        return;
      case 'step':
        setPercent(0);
        say(`step ${event.step + 1}: ${event.data.type}`);
        return;
      case 'flashProgress':
        setPercent(event.data.percent);
        setStatus(
          `${event.data.percent.toFixed(1)}% - ${(event.data.rate / 1024).toFixed(1)} mb/s - eta ${duration(
            event.data.eta,
          )}`,
        );
        return;
    }
  };

  const awaitGesture = (reason: GestureReason): Promise<void> => {
    if (reason === 'reconnect') {
      say('it came back as a new usb device, listed as an unnamed amlogic one.', 'warn');
      say('click select and pick it again to keep going.', 'warn');
    } else {
      say('click select and pick the car thing in the browser prompt.');
    }
    setStatus('waiting for you to select the device');
    return new Promise<void>(resolve => {
      setGesture(() => () => {
        setGesture(null);
        setStatus('connecting');
        resolve();
      });
    });
  };

  const flash = async () => {
    const build = builds.find(b => b.slug === selected);
    if (!build) return;
    setRunning(true);
    setPercent(0);

    try {
      say(`selected ${build.version} (${mb(build.size)})`);
      setStatus('downloading');

      let lastLogged = 0;
      const { blob, source, cached } = await loadBundle(build.url, build.size, (received, total) => {
        setPercent(total ? (received / total) * 100 : 0);
        setStatus(`downloading ${mb(received)}${total ? ` / ${mb(total)}` : ''}`);
        if (received - lastLogged > LOG_EVERY_BYTES) {
          lastLogged = received;
          say(`downloaded ${mb(received)}`);
        }
      });

      if (source === 'cache') {
        say(`reusing the cached download, ${mb(blob.size)}`, 'ok');
      } else {
        say(`bundle ready, ${mb(blob.size)}`, 'ok');
        say(
          cached
            ? 'cached for next time, so a retry will not download it again'
            : 'could not cache it: a retry will download it again',
          cached ? 'info' : 'warn',
        );
      }

      setStatus('connecting');
      const handle = await prepareFlash(blob, onEvent, awaitGesture);
      say(`config loaded, ${handle.steps} steps`, 'ok');

      setStatus('flashing');
      await handle.run();

      setPercent(100);
      setStatus('done');
      say('flash complete. unplug and replug the thing to boot it.', 'ok');
    } catch (err) {
      setStatus('failed');
      setGesture(null);
      say(err instanceof Error ? err.message : String(err), 'err');
      say('the download is cached, so pressing flash again picks up from the device step.');
    } finally {
      setRunning(false);
    }
  };

  return (
    <div class="flex flex-col gap-8">
      <div class="flex flex-col gap-4">
        <p class="text-accent m-0 font-mono text-sm">2 - pick a build</p>
        {builds.length === 0 ? (
          <div class="border border-dashed border-white/25 p-6">
            <p class="m-0 font-mono text-sm text-white/60">
              the manifest has no published builds right now, so there is nothing to flash.
            </p>
          </div>
        ) : (
          <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
            {builds.map(build => (
              <label
                key={build.slug}
                class={`relative flex cursor-pointer flex-col gap-2 border border-dashed border-white/25 p-6 pt-7 ${
                  selected === build.slug ? 'border-accent border-solid' : ''
                }`}>
                <input
                  type="radio"
                  name="channel"
                  value={build.slug}
                  checked={selected === build.slug}
                  disabled={running}
                  onChange={() => setSelected(build.slug)}
                  class="sr-only"
                />
                <span class="bg-bg absolute -top-2.5 left-4 px-2 font-mono text-sm text-white/45">{build.slug}</span>
                <span class="flex flex-wrap items-center gap-3">
                  <span class="font-display text-xl font-medium tracking-tight">{build.name}</span>
                  <span class={`pill ${build.stability === 'stable' ? 'pill-stable' : 'pill-experimental'}`}>
                    {build.stability}
                  </span>
                </span>
                <span class="text-base text-pretty text-white/60">{build.description}</span>
                <span class="font-mono text-sm text-white/40">
                  <code class="text-white/70">{build.version}</code>
                </span>
              </label>
            ))}
          </div>
        )}
      </div>

      <div class="flex flex-col gap-4">
        <p class="text-accent m-0 font-mono text-sm">3 - flash it</p>
        <div class="flex flex-wrap items-center gap-4">
          <button
            type="button"
            class="btn btn-primary"
            disabled={running || builds.length === 0}
            onClick={() => void flash()}>
            flash
          </button>
          {gesture ? (
            <button type="button" class="btn" onClick={gesture}>
              select
            </button>
          ) : null}
          <p class="m-0 font-mono text-sm text-white/50">{status}</p>
        </div>
        <div class="h-1.5 w-full bg-white/10">
          <div
            class="bg-accent h-full transition-[width] duration-200"
            style={{ width: `${Math.max(0, Math.min(100, percent)).toFixed(1)}%` }}
          />
        </div>
      </div>

      <ConsoleLog title="/var/log/flash" lines={lines} />
    </div>
  );
}
