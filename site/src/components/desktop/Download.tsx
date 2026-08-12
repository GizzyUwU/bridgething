import type { VNode } from 'preact';
import { useEffect, useState } from 'preact/hooks';

import type { DesktopRow } from '../../lib/desktop';

type Props = {
  rows: DesktopRow[];
};

type HighEntropy = { architecture?: string; bitness?: string };

type UserAgentData = {
  getHighEntropyValues?: (hints: string[]) => Promise<HighEntropy>;
};

export function Download({ rows }: Props): VNode {
  const [target, setTarget] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    detect().then(found => {
      if (live) setTarget(found);
    });
    return () => {
      live = false;
    };
  }, []);

  if (!target) {
    return <p class="m-0 font-mono text-sm text-white/35">pick the build that matches your machine below</p>;
  }

  const row = rows.find(candidate => candidate.target === target);
  if (!row?.build) {
    return <p class="m-0 font-mono text-sm text-white/35">no build for {target} yet. the rest are below.</p>;
  }

  return (
    <div class="flex flex-wrap items-center gap-x-6 gap-y-3">
      <a href={row.build.url} class="btn btn-primary" rel="noopener">
        download for {row.os} {row.arch}
      </a>
      <span class="font-mono text-sm text-white/40">{`${row.target} · ${row.artifact}`}</span>
    </div>
  );
}

async function detect(): Promise<string | null> {
  return detectTarget(navigator.userAgent, await architectureHint());
}

export function detectTarget(ua: string, architecture?: string): string | null {
  if (/Android|iPhone|iPad|iPod/i.test(ua)) return null;

  const os = /Windows/i.test(ua)
    ? 'windows'
    : /Mac OS X|Macintosh/i.test(ua)
      ? 'darwin'
      : /Linux|X11/i.test(ua)
        ? 'linux'
        : null;
  if (!os) return null;

  if (architecture === 'arm') return `${os}-aarch64`;
  if (architecture === 'x86') return `${os}-x86_64`;
  if (/aarch64|arm64/i.test(ua)) return `${os}-aarch64`;
  return `${os}-${os === 'darwin' ? 'aarch64' : 'x86_64'}`;
}

async function architectureHint(): Promise<string | undefined> {
  const data = (navigator as Navigator & { userAgentData?: UserAgentData }).userAgentData;
  if (!data?.getHighEntropyValues) return undefined;
  try {
    return (await data.getHighEntropyValues(['architecture'])).architecture;
  } catch {
    return undefined;
  }
}
