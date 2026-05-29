import { AppWindow } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import { Image, Text, View } from 'react-native';
import { SvgXml } from 'react-native-svg';

import { getSession } from '../lib/session';

type IconData = { svg?: string; fileUri?: string };

/** Fetches and renders a webapp icon. Vector icons arrive as inline markup
 *  (rendered via SvgXml); raster icons arrive as a cached file uri. Falls back
 *  to the name initial, or a generic glyph when there is no name. */
export function WebappIcon({
  deviceId,
  id,
  iconAvailable,
  name,
  size,
  radiusClass = 'rounded-xl',
  fallbackTextClass = 'text-[18px] font-extrabold text-foreground',
}: {
  deviceId: string;
  id: string;
  iconAvailable: boolean;
  name: string;
  size: number;
  radiusClass?: string;
  fallbackTextClass?: string;
}) {
  const session = getSession();
  const [icon, setIcon] = useState<IconData | null>(null);

  useEffect(() => {
    if (!iconAvailable) {
      setIcon(null);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const result = await session.webappIcon(deviceId, id);
        if (!cancelled && result) {
          setIcon({ svg: result.svg, fileUri: result.fileUri });
        }
      } catch {
        // icon load failure is non-fatal; the fallback renders.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [deviceId, iconAvailable, id, session]);

  const dims = { width: size, height: size };

  return (
    <View
      className={`items-center justify-center overflow-hidden bg-secondary ${radiusClass}`}
      style={dims}
    >
      {icon?.svg ? (
        <SvgXml xml={icon.svg} width={size} height={size} />
      ) : icon?.fileUri ? (
        <Image source={{ uri: icon.fileUri }} style={dims} resizeMode="cover" />
      ) : name ? (
        <Text className={fallbackTextClass}>
          {name.slice(0, 1).toUpperCase()}
        </Text>
      ) : (
        <AppWindow
          size={Math.round(size * 0.42)}
          color="hsl(215 14% 38%)"
          strokeWidth={2.2}
        />
      )}
    </View>
  );
}
