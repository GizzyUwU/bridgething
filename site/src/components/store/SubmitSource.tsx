import { useState } from 'preact/hooks';
import { DirectoryApiError, submitSource, type DirectoryEntry } from '../../lib/directory-client';

type Outcome = { kind: 'ok' | 'err'; message: string } | null;

export function SubmitSource({ onSubmitted }: { onSubmitted: (entry: DirectoryEntry) => void }) {
  const [url, setUrl] = useState('');
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<Outcome>(null);

  async function onSubmit(event: Event) {
    event.preventDefault();
    const candidate = url.trim();
    if (!candidate || busy) return;

    setBusy(true);
    setOutcome(null);

    try {
      const entry = await submitSource(candidate);
      setUrl('');
      setOutcome({
        kind: 'ok',
        message:
          entry.status === 'quarantined'
            ? `${entry.name} is in the directory as unreviewed. it shows up under "unreviewed" below; it reaches the phone app once someone looks at it.`
            : `${entry.name} is already in the directory as ${entry.status}, and its details were refreshed.`,
      });
      onSubmitted(entry);
    } catch (err) {
      setOutcome({
        kind: 'err',
        message: err instanceof DirectoryApiError ? err.message : `submitting failed: ${String(err)}`,
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <details class="mb-10 border border-white/15 p-4">
      <summary class="cursor-pointer font-medium">submit a source</summary>

      <p class="mt-2 mb-4 max-w-2xl text-sm text-white/60">
        publish a <code>catalog.v1</code> document at a stable https url and paste it here. submissions are checked
        automatically for being reachable, parsing as <code>catalog.v1</code>, and sending{' '}
        <code>Access-Control-Allow-Origin</code>. <a href="/docs/publishing-apps">publishing docs</a>.
      </p>

      <form class="flex flex-wrap gap-3" onSubmit={onSubmit}>
        <input
          type="url"
          required
          value={url}
          disabled={busy}
          onInput={event => setUrl((event.target as HTMLInputElement).value)}
          placeholder="https://example.com/catalog.json"
          class="min-w-0 flex-1 border border-white/25 bg-transparent px-3 py-2 font-mono text-sm text-white placeholder:text-white/30 focus:border-white/50 focus:outline-none"
        />
        <button type="submit" class="btn btn-primary" disabled={busy}>
          {busy ? 'checking…' : 'submit'}
        </button>
      </form>

      {outcome ? (
        <p class={`mt-3 text-sm ${outcome.kind === 'ok' ? 'text-ok' : 'text-warn'}`}>{outcome.message}</p>
      ) : null}
    </details>
  );
}
