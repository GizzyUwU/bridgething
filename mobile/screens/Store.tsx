import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { ChevronRight, Link as LinkIcon, Plus } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';

import { Button } from '../components/Button';
import { CatalogRow } from '../components/CatalogRow';
import { Field } from '../components/Field';
import { ListGroup } from '../components/ListGroup';
import { Press } from '../components/Press';
import { ScreenHeader } from '../components/ScreenHeader';
import { ScrollScreen } from '../components/ScrollScreen';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import {
  refreshCatalog,
  useCatalog,
  useListings,
  useQuickAddSources,
} from '../lib/catalog';
import { peerDisplayName, useSession } from '../lib/session';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'Store'>;

export function StoreScreen({ route, navigation }: Props) {
  const deviceId = route.params?.deviceId ?? null;

  const [newSource, setNewSource] = useState('');

  const sources = useCatalog(s => s.sources);
  const failures = useCatalog(s => s.failures);
  const refreshing = useCatalog(s => s.refreshing);

  const peer = useSession(s =>
    deviceId ? (s.peers.find(p => p.id === deviceId) ?? null) : null,
  );
  const ledger = useSession(s => s.ledger);

  const listings = useListings(deviceId);
  const recommended = useQuickAddSources();

  useEffect(() => {
    void refreshCatalog();
  }, []);

  const openApp = (appId: string, sourceUrl: string) =>
    navigation.navigate('StoreApp', { deviceId, appId, sourceUrl });

  const openSource = (url: string, name: string) =>
    navigation.navigate('StoreSource', { deviceId, url, name });

  const onAddSource = async () => {
    const trimmed = newSource.trim();
    if (!trimmed) return;
    setNewSource('');
    openSource(trimmed, trimmed);
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

      <SectionHeader
        title="available apps"
        action="refresh"
        actionPending={refreshing}
        onActionPress={() => void refreshCatalog()}
      />

      {refreshing && listings.length === 0 ? (
        <View className="items-center py-8">
          <ActivityIndicator />
        </View>
      ) : listings.length === 0 ? (
        <SectionEmpty>no apps available from your sources yet</SectionEmpty>
      ) : (
        <ListGroup>
          {listings.map(listing => (
            <CatalogRow
              key={listing.app.id}
              listing={listing}
              onPress={() => openApp(listing.app.id, listing.sourceUrl)}
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
        <SectionHeader title="my sources" hint="tap one to browse it" />
        {sources.length === 0 ? (
          <SectionEmpty>no catalog sources subscribed</SectionEmpty>
        ) : (
          <ListGroup>
            {sources.map(src => (
              <Press
                key={src}
                onPress={() => openSource(src, src)}
                fade={false}
                scaleTo={0.99}
              >
                <View className="flex-row items-center gap-3 px-4 py-3">
                  <Text
                    className="flex-1 text-[13px] text-foreground"
                    numberOfLines={1}
                  >
                    {src}
                  </Text>
                  <ChevronRight
                    size={16}
                    color="hsl(215 14% 60%)"
                    strokeWidth={2.4}
                  />
                </View>
              </Press>
            ))}
          </ListGroup>
        )}

        {recommended.length > 0 ? (
          <View className="mt-6">
            <SectionHeader
              title="suggested sources"
              hint="browse before you add"
            />
            <ListGroup>
              {recommended.map(source => (
                <Press
                  key={source.url}
                  onPress={() => openSource(source.url, source.name)}
                  fade={false}
                  scaleTo={0.99}
                >
                  <View className="flex-row items-center gap-3 px-4 py-3">
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
                    <ChevronRight
                      size={16}
                      color="hsl(215 14% 60%)"
                      strokeWidth={2.4}
                    />
                  </View>
                </Press>
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
            label="browse a source by url"
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
            browse source
          </Button>
        </View>
      </View>
    </ScrollScreen>
  );
}
