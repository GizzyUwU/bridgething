import type {
  CatalogAppListing,
  RecommendedSource,
} from '@bridgething/catalog';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { Link as LinkIcon, Plus, RefreshCw, Trash2 } from 'lucide-react-native';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { ActivityIndicator, Alert, Image, Text, View } from 'react-native';

import { Button } from '../components/Button';
import { Field } from '../components/Field';
import { ListGroup } from '../components/ListGroup';
import { Press } from '../components/Press';
import { ScreenHeader } from '../components/ScreenHeader';
import { ScrollScreen } from '../components/ScrollScreen';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import {
  addSource,
  installApp,
  listingsFor,
  quickAddSources,
  refreshCatalog,
  removeSource,
  useCatalog,
} from '../lib/catalog';
import { peerDisplayName, useSession } from '../lib/session';
import { useWebapps } from '../lib/webapps';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Store'>;

export function StoreScreen({ route }: Props) {
  const deviceId = route.params?.deviceId ?? null;

  const [newSource, setNewSource] = useState('');
  const [busyId, setBusyId] = useState<string | null>(null);

  const sources = useCatalog(s => s.sources);
  const catalogs = useCatalog(s => s.catalogs);
  const directory = useCatalog(s => s.directory);
  const failures = useCatalog(s => s.failures);
  const refreshing = useCatalog(s => s.refreshing);

  const peer = useSession(s =>
    deviceId ? (s.peers.find(p => p.id === deviceId) ?? null) : null,
  );
  const ledger = useSession(s => s.ledger);
  const installed = useWebapps(deviceId ?? '');

  const listings = useMemo(
    () => listingsFor(deviceId),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [deviceId, catalogs, installed.list],
  );
  const recommended = useMemo<RecommendedSource[]>(
    () => quickAddSources(),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [catalogs, directory, sources],
  );

  useEffect(() => {
    void refreshCatalog();
  }, []);

  const install = useCallback(
    (listing: CatalogAppListing) => {
      const version = listing.newestCompatible;
      if (!version || busyId) return;
      if (!deviceId) {
        Alert.alert(
          'No Car Thing connected',
          `connect one to install ${listing.app.name}. browsing works without it.`,
        );
        return;
      }
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
                await installApp(deviceId, listing);
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
    },
    [busyId, deviceId],
  );

  const onAddSource = async () => {
    const trimmed = newSource.trim();
    if (!trimmed) return;
    setNewSource('');
    await addSource(trimmed);
  };

  const onRemoveSource = (url: string) => {
    Alert.alert('Remove source?', url, [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Remove',
        style: 'destructive',
        onPress: () => void removeSource(url),
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
            : deviceId
              ? 'browse and install webapps onto your Car Thing.'
              : 'browse webapps. connect a Car Thing to install one.'
        }
      />

      <View className="mb-3 flex-row items-center justify-between">
        <SectionHeader title="available apps" />
        <Press
          onPress={() => void refreshCatalog()}
          scaleTo={0.9}
          disabled={refreshing}
        >
          <View className="flex-row items-center gap-1.5 px-1 py-1">
            {refreshing ? (
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

      {refreshing && listings.length === 0 ? (
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

      {failures.length > 0 ? (
        <Text className="mt-3 px-1 text-[11.5px] leading-[16px] text-muted-foreground">
          {failures.length} source{failures.length === 1 ? '' : 's'} could not
          be read. anything already installed from them keeps working.
        </Text>
      ) : null}

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
                <Press onPress={() => onRemoveSource(src)} scaleTo={0.9}>
                  <Trash2 size={18} color="hsl(0 70% 55%)" strokeWidth={2.2} />
                </Press>
              </View>
            ))}
          </ListGroup>
        )}

        {recommended.length > 0 ? (
          <View className="mt-6">
            <SectionHeader title="suggested sources" />
            <ListGroup>
              {recommended.map(source => (
                <View
                  key={source.url}
                  className="flex-row items-center gap-3 px-4 py-3"
                >
                  <View className="flex-1">
                    <View className="flex-row items-center gap-2">
                      <Text
                        className="text-[14px] font-semibold text-foreground"
                        numberOfLines={1}
                      >
                        {source.name}
                      </Text>
                      {source.attested ? (
                        <View className="rounded-full bg-primary-soft px-2 py-0.5">
                          <Text className="text-[10px] font-bold uppercase tracking-[0.08em] text-muted-foreground">
                            vouched for
                          </Text>
                        </View>
                      ) : null}
                    </View>
                    <Text
                      className="mt-0.5 text-[12px] text-muted-foreground"
                      numberOfLines={2}
                    >
                      {source.description ?? source.url}
                    </Text>
                  </View>
                  <Press
                    onPress={() => void addSource(source.url)}
                    scaleTo={0.9}
                  >
                    <View className="rounded-full bg-primary px-3 py-1.5">
                      <Text className="text-[11px] font-bold uppercase tracking-[0.08em] text-primary-foreground">
                        add
                      </Text>
                    </View>
                  </Press>
                </View>
              ))}
            </ListGroup>
            <Text className="mt-2 px-1 text-[11.5px] leading-[16px] text-muted-foreground">
              listed in the bridgething directory. a listing means someone
              checked it is a real catalog, never that its apps are safe.
            </Text>
          </View>
        ) : null}

        <View className="mt-6">
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
            onPress={onAddSource}
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
  listing: CatalogAppListing;
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
