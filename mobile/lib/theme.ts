// Mirror of global.css palette so react-navigation's theme can match
// NativeWind. Keep both in sync — one canonical source on each side
// because NativeWind doesn't expose css vars to JS at runtime.
//
// Brand source: docs/brand.md
// charcoal:  #1B1F23 → 210 14% 12%
// off-white: #EFEFEF →   0  0% 94%
// blue:      #00A8E8 → 199 100% 46%
// soft-gray: #A7ADB5 → 215  7% 68%

export const PALETTE = {
  light: {
    background: 'hsl(0 0% 96%)',
    foreground: 'hsl(210 14% 12%)',
    surface: 'hsl(0 0% 100%)',
    surfaceForeground: 'hsl(210 14% 12%)',
    surfaceSubtle: 'hsl(210 12% 94%)',
    card: 'hsl(0 0% 100%)',
    cardForeground: 'hsl(210 14% 12%)',
    primary: 'hsl(199 100% 46%)',
    primaryForeground: 'hsl(0 0% 100%)',
    primarySoft: 'hsl(199 100% 94%)',
    secondary: 'hsl(210 12% 92%)',
    secondaryForeground: 'hsl(210 14% 14%)',
    muted: 'hsl(210 12% 92%)',
    mutedForeground: 'hsl(215 8% 46%)',
    destructive: 'hsl(0 72% 50%)',
    destructiveForeground: 'hsl(0 0% 100%)',
    destructiveSoft: 'hsl(0 72% 96%)',
    success: 'hsl(152 60% 38%)',
    successForeground: 'hsl(0 0% 100%)',
    successSoft: 'hsl(152 60% 95%)',
    border: 'hsl(215 7% 86%)',
    borderStrong: 'hsl(215 7% 78%)',
  },
  dark: {
    background: 'hsl(210 14% 9%)',
    foreground: 'hsl(0 0% 96%)',
    surface: 'hsl(210 14% 13%)',
    surfaceForeground: 'hsl(0 0% 96%)',
    surfaceSubtle: 'hsl(210 14% 11%)',
    card: 'hsl(210 14% 13%)',
    cardForeground: 'hsl(0 0% 96%)',
    primary: 'hsl(199 100% 56%)',
    primaryForeground: 'hsl(210 14% 8%)',
    primarySoft: 'hsl(199 80% 18%)',
    secondary: 'hsl(210 12% 20%)',
    secondaryForeground: 'hsl(0 0% 96%)',
    muted: 'hsl(210 12% 20%)',
    mutedForeground: 'hsl(215 7% 68%)',
    destructive: 'hsl(0 80% 64%)',
    destructiveForeground: 'hsl(0 0% 100%)',
    destructiveSoft: 'hsl(0 60% 18%)',
    success: 'hsl(152 60% 50%)',
    successForeground: 'hsl(210 14% 8%)',
    successSoft: 'hsl(152 50% 14%)',
    border: 'hsl(210 14% 22%)',
    borderStrong: 'hsl(210 14% 30%)',
  },
};
