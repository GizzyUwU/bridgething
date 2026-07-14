import type {
  BluetoothPin,
  Notification,
  PeerSnapshotMap,
  PhoneCall,
  PhoneCallStatus,
  VolumeChanged,
} from '@bridgething/client';
import { BridgethingClient } from '@bridgething/client';

import { el, mountRoot, type OverlayRoot } from './ui';

export type OverlaySurfaces = {
  notifications: boolean;
  call: boolean;
  pairing: boolean;
  connection: boolean;
  volume: boolean;
};

export type OverlayConfig = { origin: string; surfaces: OverlaySurfaces };

declare global {
  interface Window {
    __bridgethingOverlay?: OverlayConfig;
    __bridgethingOverlayMounted?: boolean;
  }
}

const TOAST_TTL_MS = 5_000;
const TOAST_MAX = 3;
const VOLUME_TTL_MS = 1_500;
const CONNECTION_SHOW_DELAY_MS = 3_000;

const CALL_LABEL: Partial<Record<PhoneCallStatus, string>> = {
  ringing: 'incoming call',
  connecting: 'connecting',
  sending: 'calling',
  active: 'on call',
  held: 'on hold',
};

function boot() {
  const cfg = window.__bridgethingOverlay;
  if (!cfg || !location.origin.startsWith(cfg.origin)) return;
  if (window.__bridgethingOverlayMounted) return;
  window.__bridgethingOverlayMounted = true;
  const mount = () => new Overlay(cfg, mountRoot(), new BridgethingClient({ url: `ws://${location.host}/` }));
  if (document.body) mount();
  else document.addEventListener('DOMContentLoaded', mount, { once: true });
}

export class Overlay {
  private readonly root: OverlayRoot;
  private readonly dismissers: Array<() => void> = [];

  constructor(cfg: OverlayConfig, root: OverlayRoot, client: BridgethingClient) {
    this.root = root;
    if (cfg.surfaces.connection) this.wireConnection(client);
    if (cfg.surfaces.call) this.wireCall(client);
    if (cfg.surfaces.notifications) this.wireNotifications(client);
    if (cfg.surfaces.pairing) this.wirePairing(client);
    if (cfg.surfaces.volume) this.wireVolume(client);
    document.addEventListener(
      'keydown',
      event => {
        if (event.key !== 'Escape' || this.dismissers.length === 0) return;
        event.stopPropagation();
        event.preventDefault();
        this.dismissers.pop()?.();
      },
      { capture: true },
    );
  }

  private dismissible(hide: () => void): () => void {
    const entry = () => hide();
    this.dismissers.push(entry);
    return () => {
      const at = this.dismissers.indexOf(entry);
      if (at >= 0) this.dismissers.splice(at, 1);
    };
  }

  private wireConnection(client: BridgethingClient) {
    const banner = el('div', 'banner connection hidden');
    banner.append(el('span', 'dot'), el('span', 'title', 'phone disconnected'));
    this.root.top.appendChild(banner);
    let showTimer: ReturnType<typeof setTimeout> | undefined;
    client.peer.onSnapshot((snapshot: PeerSnapshotMap) => {
      const peers = Object.values(snapshot);
      const anyPaired = peers.some(p => p.paired);
      const anyUseful = peers.some(p => p.iap2 === 'identified' || p.companion.type === 'connected');
      if (anyPaired && !anyUseful) {
        showTimer ??= setTimeout(() => banner.classList.remove('hidden'), CONNECTION_SHOW_DELAY_MS);
      } else {
        clearTimeout(showTimer);
        showTimer = undefined;
        banner.classList.add('hidden');
      }
    });
  }

  private wireCall(client: BridgethingClient) {
    const banner = el('div', 'banner call hidden');
    const title = el('span', 'title');
    const sub = el('span', 'sub');
    banner.append(el('span', 'dot'), title, sub);
    this.root.top.appendChild(banner);
    let shown: string | undefined;
    let undismiss: (() => void) | undefined;
    const hide = () => {
      shown = undefined;
      banner.classList.add('hidden');
      undismiss?.();
      undismiss = undefined;
    };
    const show = (call: PhoneCall) => {
      const label = CALL_LABEL[call.status];
      if (!label) return hide();
      shown = call.callId;
      title.textContent = call.displayName || call.remoteId || 'unknown caller';
      sub.textContent = label;
      banner.classList.toggle('ringing', call.status === 'ringing');
      banner.classList.remove('hidden');
      undismiss ??= this.dismissible(hide);
    };
    client.phone.onCallStarted(show);
    client.phone.onCallUpdated(call => {
      if (shown === undefined || shown === call.callId) show(call);
    });
    client.phone.onCallEnded(ended => {
      if (shown === ended.callId) hide();
    });
  }

  private wireNotifications(client: BridgethingClient) {
    const live = new Map<string, { node: HTMLElement; timer: ReturnType<typeof setTimeout> }>();
    const drop = (id: string) => {
      const entry = live.get(id);
      if (!entry) return;
      clearTimeout(entry.timer);
      entry.node.remove();
      live.delete(id);
    };
    const post = (n: Notification) => {
      if (n.flags.silent || n.flags.preExisting) return;
      drop(n.id);
      const node = el('div', 'toast');
      node.append(el('div', 'app', n.app.displayName ?? n.app.bundleId), el('div', 'title', n.title ?? ''));
      const msg = n.message ?? n.subtitle;
      if (msg) node.appendChild(el('div', 'msg', msg));
      this.root.toasts.appendChild(node);
      live.set(n.id, { node, timer: setTimeout(() => drop(n.id), TOAST_TTL_MS) });
      while (live.size > TOAST_MAX) {
        const oldest = live.keys().next().value;
        if (oldest === undefined) break;
        drop(oldest);
      }
    };
    client.notifications.onPosted(post);
    client.notifications.onUpdated(post);
    client.notifications.onRemoved(removed => drop(removed.id));
  }

  private wirePairing(client: BridgethingClient) {
    let scrim: HTMLElement | undefined;
    let undismiss: (() => void) | undefined;
    const hide = () => {
      scrim?.remove();
      scrim = undefined;
      undismiss?.();
      undismiss = undefined;
    };
    client.bluetooth.onPin((pin: BluetoothPin) => {
      hide();
      scrim = el('div', 'scrim');
      const modal = el('div', 'modal');
      modal.append(
        el('div', 'head', 'enter this pin on your phone'),
        el('div', 'pin', pin.pin),
        el('div', 'name', pin.name || pin.mac),
      );
      scrim.appendChild(modal);
      this.root.layer.appendChild(scrim);
      undismiss = this.dismissible(hide);
    });
    client.bluetooth.onPairingResult(() => hide());
  }

  private wireVolume(client: BridgethingClient) {
    const bar = el('div', 'volume hidden');
    const label = el('span', 'label');
    const track = el('div', 'track');
    const fill = el('div', 'fill');
    track.appendChild(fill);
    bar.append(label, track);
    this.root.layer.appendChild(bar);
    let hideTimer: ReturnType<typeof setTimeout> | undefined;
    client.audio.onVolumeChanged((v: VolumeChanged) => {
      const percent = Math.round(Math.min(1, Math.max(0, v.level)) * 100);
      label.textContent = v.muted ? 'muted' : `${percent}%`;
      fill.style.width = `${v.muted ? 0 : percent}%`;
      bar.classList.toggle('muted', v.muted);
      bar.classList.remove('hidden');
      clearTimeout(hideTimer);
      hideTimer = setTimeout(() => bar.classList.add('hidden'), VOLUME_TTL_MS);
    });
  }
}

boot();
