import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { Check, Plus, Trash2 } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';

import { Button } from '../components/Button';
import { CatalogRow } from '../components/CatalogRow';
import { ListGroup } from '../components/ListGroup';
import { ScrollScreen } from '../components/ScrollScreen';
import { SectionEmpty, SectionHeader } from '../components/SectionHeader';
import {
  addSource,
  previewSource,
  removeSource,
  useIsSubscribed,
  useSourceListings,
} from '../lib/catalog';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'StoreSource'>;

export function StoreSourceScreen({ route, navigation }: Props) {
  const { deviceId, url, name } = route.params;
  const subscribed = useIsSubscribed(url);
  const listings = useSourceListings(url, deviceId);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    previewSource(url)
      .then(() => {
        if (!cancelled) setError(null);
      })
      .catch(err => {
        if (!cancelled)
          setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [url]);

  return (
    <ScrollScreen>
      <View className="mb-6">
        <Text className="text-[20px] font-extrabold text-foreground">
          {name}
        </Text>
        <Text
          className="mt-0.5 text-[12px] text-muted-foreground"
          numberOfLines={2}
        >
          {url}
        </Text>
      </View>

      <View className="mb-8">
        {subscribed ? (
          <Button
            onPress={() => void removeSource(url)}
            variant="secondary"
            icon={Trash2}
          >
            remove from my sources
          </Button>
        ) : (
          <Button onPress={() => void addSource(url)} icon={Plus}>
            add to my sources
          </Button>
        )}
        <Text className="mt-2 px-1 text-[11.5px] leading-[16px] text-muted-foreground">
          {subscribed
            ? 'its apps show up in the store and are checked for updates.'
            : 'you can install from here without adding it. adding it means its apps show up in the store and get update checks.'}
        </Text>
      </View>

      <SectionHeader title="apps" />
      {error ? (
        <View className="mb-3 rounded-2xl border border-destructive/30 bg-destructive-soft px-4 py-3">
          <Text className="text-[12px] text-destructive">{error}</Text>
        </View>
      ) : null}
      {loading && listings.length === 0 ? (
        <View className="items-center py-8">
          <ActivityIndicator />
        </View>
      ) : listings.length > 0 ? (
        <ListGroup>
          {listings.map(listing => (
            <CatalogRow
              key={listing.app.id}
              listing={listing}
              onPress={() =>
                navigation.navigate('StoreApp', {
                  deviceId,
                  appId: listing.app.id,
                  sourceUrl: url,
                })
              }
            />
          ))}
        </ListGroup>
      ) : error ? null : (
        <SectionEmpty>this source publishes no apps</SectionEmpty>
      )}

      {subscribed ? (
        <View className="mt-6 flex-row items-center gap-2 px-1">
          <Check size={14} color="hsl(215 14% 50%)" strokeWidth={2.4} />
          <Text className="text-[12px] text-muted-foreground">
            in your sources
          </Text>
        </View>
      ) : null}
    </ScrollScreen>
  );
}
