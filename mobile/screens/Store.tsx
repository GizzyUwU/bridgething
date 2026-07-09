import type { CatalogListing } from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { Link as LinkIcon, Plus, RefreshCw, Trash2 } from 'lucide-react-native';
import { useCallback, useEffect, useState } from 'react';
import { ActivityIndicator, Alert, Image, Text, View } from 'react-native';

import { Button } from '../components/Button';
import { Field } from '../components/Field';
import { ListGroup } from '../components/ListGroup';
import { Press } from '../components/Press';
import { ScreenHeader } from '../components/ScreenHeader';
import { ScrollScreen } from '../components/ScrollScreen';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import { getSession, peerDisplayName, useSession } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Store'>;

export function StoreScreen({ route }: Props) {
  const session = getSession();
  const deviceId = route.params.deviceId;

  const [listings, setListings] = useState<CatalogListing[]>([]);
  const [sources, setSources] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [newSource, setNewSource] = useState('');
  const [busyId, setBusyId] = useState<string | null>(null);

  const peer = useSession(s => s.peers.find(p => p.id === deviceId) ?? null);
  const ledger = useSession(s => s.ledger);

  const reload = useCallback(async () => {
    try {
      const [apps, srcs] = await Promise.all([
        session.availableApps(deviceId),
        session.catalogSources(),
      ]);
      setListings(apps);
      setSources(srcs);
    } catch {
      // surfaces as an empty list
    }
  }, [deviceId, session]);

  const refreshCatalog = useCallback(async () => {
    setLoading(true);
    try {
      await session.refreshCatalog();
      await reload();
    } finally {
      setLoading(false);
    }
  }, [reload, session]);

  useEffect(() => {
    refreshCatalog();
  }, [refreshCatalog]);

  useEffect(() => {
    return session.subscribe(event => {
      if (event.type === 'catalogEvent') {
        const k = event.event.kind;
        if (k === 'refreshed' || k === 'installed' || k === 'installFailed')
          void reload();
      } else if (
        event.type === 'webappsChanged' &&
        event.deviceId === deviceId
      ) {
        void reload();
      }
    });
  }, [deviceId, reload, session]);

  const install = (listing: CatalogListing) => {
    const version = listing.newestCompatible;
    if (!version || busyId) return;
    const perms = version.permissions.length
      ? version.permissions.join(', ')
      : 'none';
    Alert.alert(
      `Install ${listing.app.name}?`,
      `Version ${version.version} by ${listing.app.author}.\n\nPermissions: ${perms}\n\nSource: ${listing.sourceUrl}`,
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Install',
          onPress: async () => {
            setBusyId(listing.app.id);
            try {
              await session.installCatalogApp(
                deviceId,
                listing.app.id,
                version.version,
                listing.sourceUrl,
              );
            } catch (err) {
              Alert.alert(
                'Install failed',
                err instanceof Error ? err.message : String(err),
              );
            } finally {
              setBusyId(null);
            }
          },
        },
      ],
    );
  };

  const addSource = async () => {
    const trimmed = newSource.trim();
    if (!trimmed) return;
    setNewSource('');
    await session.addCatalogSource(trimmed);
    await refreshCatalog();
  };

  const removeSource = (url: string) => {
    Alert.alert('Remove source?', url, [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Remove',
        style: 'destructive',
        onPress: async () => {
          await session.removeCatalogSource(url);
          await refreshCatalog();
        },
      },
    ]);
  };

  return (
    <ScrollScreen>
      <ScreenHeader
        title="app store"
        subtitle={
          peer
            ? `browse and install webapps onto ${peerDisplayName(peer, ledger)}.`
            : 'browse and install webapps onto your Car Thing.'
        }
      />

      <View className="mb-3 flex-row items-center justify-between">
        <SectionHeader title="available apps" />
        <Press onPress={refreshCatalog} scaleTo={0.9} disabled={loading}>
          <View className="flex-row items-center gap-1.5 px-1 py-1">
            {loading ? (
              <ActivityIndicator size="small" />
            ) : (
              <RefreshCw size={15} color="hsl(215 14% 45%)" strokeWidth={2.4} />
            )}
            <Text className="text-[13px] font-semibold text-muted-foreground">
              refresh
            </Text>
          </View>
        </Press>
      </View>

      {loading && listings.length === 0 ? (
        <View className="items-center py-8">
          <ActivityIndicator />
        </View>
      ) : listings.length === 0 ? (
        <SectionEmpty>no apps available from your sources yet</SectionEmpty>
      ) : (
        <ListGroup>
          {listings.map(listing => (
            <AppRow
              key={listing.app.id}
              listing={listing}
              busy={busyId === listing.app.id}
              onInstall={() => install(listing)}
            />
          ))}
        </ListGroup>
      )}

      <View className="mt-10">
        <SectionHeader title="sources" />
        {sources.length === 0 ? (
          <SectionEmpty>no catalog sources subscribed</SectionEmpty>
        ) : (
          <ListGroup>
            {sources.map(src => (
              <View key={src} className="flex-row items-center gap-3 px-4 py-3">
                <Text
                  className="flex-1 text-[13px] text-foreground"
                  numberOfLines={1}
                >
                  {src}
                </Text>
                <Press onPress={() => removeSource(src)} scaleTo={0.9}>
                  <Trash2 size={18} color="hsl(0 70% 55%)" strokeWidth={2.2} />
                </Press>
              </View>
            ))}
          </ListGroup>
        )}
        <View className="mt-3">
          <Field
            label="add a source"
            icon={LinkIcon}
            value={newSource}
            onChangeText={setNewSource}
            clearable
            placeholder="https://example.com/catalog.json"
            autoCapitalize="none"
            autoCorrect={false}
            keyboardType="url"
          />
        </View>
        <View className="mt-3">
          <Button
            onPress={addSource}
            disabled={newSource.trim().length === 0}
            icon={Plus}
            size="lg"
          >
            add source
          </Button>
        </View>
      </View>
    </ScrollScreen>
  );
}

function AppRow({
  listing,
  busy,
  onInstall,
}: {
  listing: CatalogListing;
  busy: boolean;
  onInstall: () => void;
}) {
  const {
    app,
    newestCompatible,
    installedVersion,
    updateAvailable,
    alsoAvailableFrom,
  } = listing;
  const incompatible = !newestCompatible;
  const cta = incompatible
    ? 'incompatible'
    : installedVersion
      ? updateAvailable
        ? 'update'
        : 'installed'
      : 'install';
  const tappable = !incompatible && (!installedVersion || updateAvailable);

  return (
    <Press
      onPress={tappable && !busy ? onInstall : undefined}
      fade={false}
      scaleTo={tappable ? 0.99 : 1}
    >
      <View className="flex-row items-center gap-3 px-4 py-3.5">
        {app.icon ? (
          <Image source={{ uri: app.icon }} className="h-11 w-11 rounded-xl" />
        ) : (
          <View className="h-11 w-11 items-center justify-center rounded-xl bg-secondary">
            <Text className="text-[16px] font-extrabold text-foreground">
              {app.name.charAt(0).toUpperCase()}
            </Text>
          </View>
        )}
        <View className="flex-1">
          <Text
            className="text-[15px] font-semibold text-foreground"
            numberOfLines={1}
          >
            {app.name}
          </Text>
          <Text
            className="mt-0.5 text-[12.5px] text-muted-foreground"
            numberOfLines={2}
          >
            {app.description}
          </Text>
          <Text
            className="mt-0.5 text-[11px] text-muted-foreground"
            numberOfLines={1}
          >
            {newestCompatible
              ? `v${newestCompatible.version}`
              : 'needs newer firmware'}
            {installedVersion ? ` · installed v${installedVersion}` : ''}
            {alsoAvailableFrom.length
              ? ` · also in ${alsoAvailableFrom.length} other`
              : ''}
          </Text>
        </View>
        {busy ? (
          <ActivityIndicator size="small" />
        ) : (
          <View
            className={`rounded-full px-2.5 py-1 ${updateAvailable ? 'bg-primary' : installedVersion ? 'bg-secondary' : incompatible ? 'bg-secondary' : 'bg-primary-soft'}`}
          >
            <Text
              className={`text-[11px] font-bold uppercase tracking-[0.08em] ${updateAvailable ? 'text-primary-foreground' : 'text-muted-foreground'}`}
            >
              {cta}
            </Text>
          </View>
        )}
      </View>
    </Press>
  );
}
