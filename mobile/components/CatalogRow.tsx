import type { CatalogAppListing } from '@bridgething/catalog';
import { Text, View } from 'react-native';

import { CatalogIcon } from './CatalogIcon';
import { Pill } from './Pill';
import { Press } from './Press';
import { TEXT } from '../lib/theme';
import { listingState } from '../lib/tone';

export function CatalogRow({
  listing,
  sourceName,
  onPress,
  onPressSource,
}: {
  listing: CatalogAppListing;
  sourceName?: string;
  onPress: () => void;
  onPressSource?: () => void;
}) {
  const { app, newestCompatible, installedVersion, alsoAvailableFrom } =
    listing;
  const state = listingState(listing);

  const traits = [
    newestCompatible?.role === 'launcher' ? 'home screen' : null,
    newestCompatible?.provides_overlay ? 'overlay' : null,
  ].filter(Boolean);

  return (
    <Press onPress={onPress}>
      <View className="flex-row items-center gap-3 px-4 py-3">
        <CatalogIcon url={app.icon} name={app.name} size={44} />
        <View className="min-w-0 flex-1">
          <Text
            className="font-sans text-fg"
            style={TEXT.row}
            numberOfLines={1}
          >
            {app.name}
          </Text>
          <Text
            className="mt-0.5 font-sans text-muted"
            style={TEXT.hint}
            numberOfLines={2}
          >
            {app.description}
          </Text>
          <View className="mt-1 flex-row items-center gap-1.5">
            {sourceName ? (
              <Press
                onPress={onPressSource}
                disabled={!onPressSource}
                hitSlop={6}
              >
                <Text
                  className={`font-mono ${onPressSource ? 'text-accent' : 'text-dim'}`}
                  style={TEXT.eyebrow}
                  numberOfLines={1}
                >
                  {sourceName}
                </Text>
              </Press>
            ) : null}
            <Text
              className="min-w-0 flex-1 font-mono text-dim"
              style={TEXT.eyebrow}
              numberOfLines={1}
            >
              {sourceName ? '· ' : ''}
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
        </View>
        <Pill tone={state.tone}>{state.label}</Pill>
        <Text className="font-mono text-dim" style={TEXT.body}>
          ›
        </Text>
      </View>
    </Press>
  );
}
