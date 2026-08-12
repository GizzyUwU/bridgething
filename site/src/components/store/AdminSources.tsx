import { useCallback, useEffect, useState } from 'preact/hooks';
import {
  DirectoryApiError,
  fetchAdminSources,
  setSourceStatus,
  type AdminEntry,
  type SourceStatus,
} from '../../lib/directory-client';

const TOKEN_KEY = 'bridgething:admin-token';

const PROMOTIONS: { status: SourceStatus; label: string }[] = [
  { status: 'attested', label: 'attest' },
  { status: 'listed', label: 'list' },
  { status: 'quarantined', label: 'quarantine' },
  { status: 'rejected', label: 'reject' },
];

function Row({
  entry,
  busy,
  onSetStatus,
}: {
  entry: AdminEntry;
  busy: boolean;
  onSetStatus: (url: string, status: SourceStatus) => void;
}) {
  return (
    <li class="border border-white/15 p-4">
      <div class="flex flex-wrap items-baseline justify-between gap-2">
        <span class="font-medium">{entry.name}</span>
        <span class="text-accent font-mono text-sm">{entry.status}</span>
      </div>

      <p class="m-0 mt-1 font-mono text-xs break-all text-white/40">{entry.url}</p>
      {entry.description ? <p class="m-0 mt-1 text-sm text-white/60">{entry.description}</p> : null}

      <p class="m-0 mt-1 font-mono text-xs text-white/35">
        {entry.app_count} app{entry.app_count === 1 ? '' : 's'} · submitted {entry.submitted_at.slice(0, 10)} · checked{' '}
        {entry.last_checked_at.slice(0, 10)}
        {entry.last_check_ok ? '' : ' · unreachable'}
        {entry.downloads_cors_ok === false ? ' · downloads not browser-readable' : ''}
      </p>

      {entry.last_check_error ? <p class="text-warn m-0 mt-1 text-xs">{entry.last_check_error}</p> : null}
      {entry.note ? <p class="m-0 mt-1 text-xs text-white/50">note: {entry.note}</p> : null}

      <div class="mt-3 flex flex-wrap gap-2">
        {PROMOTIONS.filter(p => p.status !== entry.status).map(p => (
          <button
            key={p.status}
            type="button"
            class="btn text-sm"
            disabled={busy}
            onClick={() => onSetStatus(entry.url, p.status)}>
            {p.label}
          </button>
        ))}
      </div>
    </li>
  );
}

export function AdminSources() {
  const [token, setToken] = useState('');
  const [entries, setEntries] = useState<AdminEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const stored = typeof localStorage === 'undefined' ? null : localStorage.getItem(TOKEN_KEY);
    if (stored) setToken(stored);
  }, []);

  const load = useCallback(async (candidate: string) => {
    if (!candidate) return;
    setBusy(true);
    setError(null);
    try {
      const sources = await fetchAdminSources(candidate);
      setEntries(sources);
      localStorage.setItem(TOKEN_KEY, candidate);
    } catch (err) {
      setEntries(null);
      setError(err instanceof DirectoryApiError ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }, []);

  const onSetStatus = useCallback(
    async (url: string, status: SourceStatus) => {
      setBusy(true);
      setError(null);
      try {
        const updated = await setSourceStatus({ token, url, status });
        setEntries(current => (current ?? []).map(entry => (entry.url === url ? updated : entry)));
      } catch (err) {
        setError(err instanceof DirectoryApiError ? err.message : String(err));
      } finally {
        setBusy(false);
      }
    },
    [token],
  );

  const grouped = (status: SourceStatus) => (entries ?? []).filter(entry => entry.status === status);

  return (
    <>
      <form
        class="mb-8 flex flex-wrap gap-3"
        onSubmit={event => {
          event.preventDefault();
          void load(token.trim());
        }}>
        <input
          type="password"
          required
          value={token}
          onInput={event => setToken((event.target as HTMLInputElement).value)}
          placeholder="admin token"
          class="min-w-0 flex-1 border border-white/25 bg-transparent px-3 py-2 font-mono text-sm text-white placeholder:text-white/30 focus:border-white/50 focus:outline-none"
        />
        <button type="submit" class="btn btn-primary" disabled={busy}>
          {busy ? 'working…' : 'load'}
        </button>
      </form>

      {error ? <p class="text-warn mb-6 text-sm">{error}</p> : null}

      {entries === null ? null : entries.length === 0 ? (
        <p class="font-mono text-sm text-white/45">nothing submitted yet.</p>
      ) : (
        (['quarantined', 'listed', 'attested', 'rejected'] as SourceStatus[]).map(status => {
          const rows = grouped(status);
          if (rows.length === 0) return null;
          return (
            <section key={status} class="mb-10">
              <header class="mb-3 flex flex-wrap items-baseline justify-between gap-3 border-b border-white/20 pb-2">
                <h2 class="m-0">{status}</h2>
                <p class="m-0 font-mono text-sm text-white/40">{rows.length}</p>
              </header>
              <ul class="grid list-none grid-cols-1 gap-4 p-0 md:grid-cols-2">
                {rows.map(entry => (
                  <Row key={entry.url} entry={entry} busy={busy} onSetStatus={onSetStatus} />
                ))}
              </ul>
            </section>
          );
        })
      )}
    </>
  );
}
