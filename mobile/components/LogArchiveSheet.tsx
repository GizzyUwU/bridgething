import type { BridgethingLogArchive } from '@bridgething/session-react-native';
import { AlertTriangle, Share2, Trash2 } from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
import { ActivityIndicator, Alert, ScrollView, Text, View } from 'react-native';

import { getSession } from '../lib/session';
import { Button } from './Button';
import { Press } from './Press';
import { Sheet } from './Sheet';

/**
 * Browser for the on-disk log launches. Each row is one app launch; the store
 * keeps the last few plus any that recorded an error, so this is where a user
 * picks the run that actually went wrong instead of exporting the lot.
 */
export function LogArchiveSheet({
  visible,
  onClose,
  onChanged,
}: {
  visible: boolean;
  onClose: () => void;
  /** Fired after a delete so the caller can refresh its on-disk size readout. */
  onChanged?: () => void;
}) {
  const [archives, setArchives] = useState<BridgethingLogArchive[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const refresh = useCallback(() => {
    getSession()
      .logArchives()
      .then(setArchives)
      .catch(() => setArchives([]));
  }, []);

  useEffect(() => {
    if (visible) {
      setArchives(null);
      refresh();
    }
  }, [visible, refresh]);

  const share = useCallback(async (archive: BridgethingLogArchive) => {
    setBusy(archive.id);
    try {
      const ok = await getSession().shareLogs(archive.id);
      if (!ok) Alert.alert('Share failed', 'No app was available to receive the log file.');
    } catch (err) {
      Alert.alert('Share failed', err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }, []);

  const confirmDelete = useCallback(
    (archive: BridgethingLogArchive) => {
      Alert.alert(
        'Delete this log?',
        archive.current
          ? 'Clears the log for the running session. New lines will keep being recorded.'
          : `Deletes the ${formatBytes(archive.bytes)} recorded on ${formatStamp(archive.startedAt)}.`,
        [
          { text: 'Cancel', style: 'cancel' },
          {
            text: 'Delete',
            style: 'destructive',
            onPress: async () => {
              setBusy(archive.id);
              try {
                await getSession().deleteLogArchive(archive.id);
              } catch {
                // a failed delete just leaves the row in place
              } finally {
                setBusy(null);
                refresh();
                onChanged?.();
              }
            },
          },
        ],
      );
    },
    [refresh, onChanged],
  );

  return (
    <Sheet visible={visible} onClose={onClose}>
      <Text
        className="text-[19px] font-bold text-foreground"
        style={{ letterSpacing: -0.4 }}
      >
        stored logs
      </Text>

      {archives === null ? (
        <View className="items-center py-8">
          <ActivityIndicator />
        </View>
      ) : archives.length === 0 ? (
        <Text className="py-6 text-center text-[13px] text-muted-foreground">
          nothing recorded on disk yet
        </Text>
      ) : (
        <ScrollView
          className="max-h-80"
          contentContainerClassName="gap-2"
          showsVerticalScrollIndicator={false}
        >
          {archives.map(archive => (
            <View
              key={archive.id}
              className="flex-row items-center gap-2 rounded-xl bg-secondary px-3 py-2.5"
            >
              <View className="flex-1">
                <View className="flex-row items-center gap-1.5">
                  <Text className="text-[13px] font-semibold text-foreground">
                    {formatStamp(archive.startedAt)}
                  </Text>
                  {archive.current ? (
                    <Text className="text-[10px] font-bold uppercase tracking-[0.1em] text-primary">
                      live
                    </Text>
                  ) : null}
                  {archive.pinned ? (
                    <AlertTriangle
                      size={11}
                      color="hsl(0 72% 50%)"
                      strokeWidth={2.6}
                    />
                  ) : null}
                </View>
                <Text className="mt-0.5 text-[11px] text-muted-foreground">
                  {formatBytes(archive.bytes)}
                  {archive.pinned ? ' · kept: has errors' : ''}
                </Text>
              </View>

              {busy === archive.id ? (
                <ActivityIndicator size="small" />
              ) : (
                <>
                  <Press
                    onPress={() => share(archive)}
                    scaleTo={0.9}
                    hitSlop={8}
                    className="rounded-full bg-primary-soft p-2"
                  >
                    <Share2
                      size={14}
                      color="hsl(199 100% 44%)"
                      strokeWidth={2.4}
                    />
                  </Press>
                  <Press
                    onPress={() => confirmDelete(archive)}
                    scaleTo={0.9}
                    hitSlop={8}
                    className="rounded-full bg-destructive-soft p-2"
                  >
                    <Trash2 size={14} color="hsl(0 72% 50%)" strokeWidth={2.4} />
                  </Press>
                </>
              )}
            </View>
          ))}
        </ScrollView>
      )}

      <Button variant="ghost" size="lg" onPress={onClose}>
        done
      </Button>
    </Sheet>
  );
}

function formatStamp(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
