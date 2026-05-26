import { Text, View } from 'react-native';

/**
 * Bridgething wordmark. Single string, lowercase, never split.
 * "bridge" in Outfit Medium, "thing" in Outfit ExtraLight.
 * Tracking -3% at display sizes, -2% in small lockups.
 * Blue is accent only, never inside the wordmark.
 */
const SIZES = {
  xs: { font: 14, tracking: -0.28, lineHeight: 16 },
  sm: { font: 18, tracking: -0.36, lineHeight: 20 },
  md: { font: 28, tracking: -0.84, lineHeight: 32 },
  lg: { font: 44, tracking: -1.32, lineHeight: 48 },
  xl: { font: 64, tracking: -1.92, lineHeight: 68 },
} as const;

type Size = keyof typeof SIZES;

export function Wordmark({
  size = 'md',
  className,
}: {
  size?: Size;
  className?: string;
}) {
  const s = SIZES[size];
  return (
    <View className={`flex-row items-end ${className ?? ''}`}>
      <Text
        className="text-foreground"
        style={{
          fontFamily: 'Outfit-Medium',
          fontSize: s.font,
          lineHeight: s.lineHeight,
          letterSpacing: s.tracking,
        }}
      >
        bridge
      </Text>
      <Text
        className="text-foreground"
        style={{
          fontFamily: 'Outfit-ExtraLight',
          fontSize: s.font,
          lineHeight: s.lineHeight,
          letterSpacing: s.tracking,
        }}
      >
        thing
      </Text>
    </View>
  );
}
