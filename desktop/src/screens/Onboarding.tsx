import { Button, ScreenHeader, StatusStrip, Wordmark } from '@bridgething/ui';
import type { VNode } from 'preact';

import { ConnectFlow } from '../components/ConnectFlow.tsx';
import { Screen } from '../components/Screen.tsx';
import { endpoints, peers } from '../stores/session.ts';

export function OnboardingScreen({ onSkip }: { onSkip: () => void }): VNode {
  const found = endpoints.data.value;
  const failed = peers.value.filter(peer => peer.status === 'linkFailed');

  return (
    <div class="flex h-full min-w-0 flex-col">
      <header class="flex shrink-0 items-center justify-between gap-3 border-b border-rule bg-screen px-6 py-4">
        <Wordmark size="sm" />
        <Button variant="ghost" size="sm" onClick={onSkip}>
          skip for now
        </Button>
      </header>

      <Screen>
        <ScreenHeader
          eyebrow="first run"
          title="connect a Car Thing"
          subtitle="this app talks to the daemon running on the device. pick one below and it takes over from here."
        />

        <StatusStrip
          tone={failed.length > 0 ? 'err' : found.length > 0 ? 'accent' : 'warn'}
          title={
            failed.length > 0
              ? 'the link did not open'
              : found.length > 0
                ? `${found.length} daemon${found.length === 1 ? '' : 's'} on your network`
                : 'looking for a Car Thing'
          }
          subtitle={
            failed.length > 0
              ? (failed[0]?.linkError ?? 'pick it again below')
              : found.length > 0
                ? 'pick one to link this computer to it'
                : 'plug one in over usb, or put it on the same network as this computer'
          }
          class="mb-6"
        />

        <ConnectFlow />
      </Screen>
    </div>
  );
}
