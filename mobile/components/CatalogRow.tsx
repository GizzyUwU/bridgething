import type { CatalogAppListing } from '@bridgething/catalog';
import { ChevronRight } from 'lucide-react-native';
import { Text, View } from 'react-native';

import { CatalogIcon } from './CatalogIcon';
import { Press } from './Press';

type ListingTone = 'update' | 'installed' | 'incompatible' | 'install';

export function listingState(listing: CatalogAppListing): {
  label: string;
  tone: ListingTone;
} {
  if (!listing.newestCompatible)
    return { label: 'incompatible', tone: 'incompatible' };
  if (listing.updateAvailable) return { label: 'update', tone: 'update' };
  if (listing.installedVersion)
    return { label: 'installed', tone: 'installed' };
  return { label: 'install', tone: 'install' };
}

const TONE_BG: Record<ListingTone, string> = {
  update: 'bg-primary',
  installed: 'bg-secondary',
  incompatible: 'bg-secondary',
  install: 'bg-primary-soft',
};

export function CatalogRow({
  listing,
  onPress,
}: {
  listing: CatalogAppListing;
  onPress: () => void;
}) {
  const { app, newestCompatible, installedVersion, alsoAvailableFrom } =
    listing;
  const state = listingState(listing);

  const traits = [
    newestCompatible?.role === 'launcher' ? 'home screen' : null,
    newestCompatible?.provides_overlay ? 'overlay' : null,
  ].filter(Boolean);

  return (
    <Press onPress={onPress} fade={false} scaleTo={0.99}>
      <View className="flex-row items-center gap-3 px-4 py-3.5">
        <CatalogIcon url={app.icon} name={app.name} size={44} />
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
            {traits.length ? ` · ${traits.join(' · ')}` : ''}
            {alsoAvailableFrom.length
              ? ` · also in ${alsoAvailableFrom.length} other source${
                  alsoAvailableFrom.length === 1 ? '' : 's'
                }`
              : ''}
          </Text>
        </View>
        <View className={`rounded-full px-2.5 py-1 ${TONE_BG[state.tone]}`}>
          <Text
            className={`text-[11px] font-bold uppercase tracking-[0.08em] ${
              state.tone === 'update'
                ? 'text-primary-foreground'
                : 'text-muted-foreground'
            }`}
          >
            {state.label}
          </Text>
        </View>
        <ChevronRight size={16} color="hsl(215 14% 60%)" strokeWidth={2.4} />
      </View>
    </Press>
  );
}
