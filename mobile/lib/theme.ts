// bridgething palette mirrors global.css. Used by react-navigation's
// theme provider so navigation chrome (header bg, tint) matches NativeWind.
//
// ink:    #1B1F23 — 210 14% 12%
// paper:  #EFEFEF —   0  0% 94%
// accent: #00A8E8 — 199 100% 46% (signature cyan in the wordmark)

export const THEME = {
  light: {
    background: 'hsl(0 0% 96%)',
    foreground: 'hsl(210 14% 12%)',
    card: 'hsl(0 0% 100%)',
    cardForeground: 'hsl(210 14% 12%)',
    popover: 'hsl(0 0% 100%)',
    popoverForeground: 'hsl(210 14% 12%)',
    primary: 'hsl(199 100% 46%)',
    primaryForeground: 'hsl(0 0% 100%)',
    secondary: 'hsl(210 14% 92%)',
    secondaryForeground: 'hsl(210 14% 12%)',
    muted: 'hsl(210 14% 92%)',
    mutedForeground: 'hsl(210 10% 40%)',
    accent: 'hsl(199 100% 46%)',
    accentForeground: 'hsl(0 0% 100%)',
    destructive: 'hsl(0 72% 51%)',
    destructiveForeground: 'hsl(0 0% 100%)',
    border: 'hsl(210 14% 86%)',
    input: 'hsl(210 14% 86%)',
    ring: 'hsl(199 100% 46%)',
    radius: '0.625rem',
  },
  dark: {
    background: 'hsl(210 14% 12%)',
    foreground: 'hsl(0 0% 94%)',
    card: 'hsl(210 14% 16%)',
    cardForeground: 'hsl(0 0% 94%)',
    popover: 'hsl(210 14% 16%)',
    popoverForeground: 'hsl(0 0% 94%)',
    primary: 'hsl(199 100% 52%)',
    primaryForeground: 'hsl(210 14% 8%)',
    secondary: 'hsl(210 14% 20%)',
    secondaryForeground: 'hsl(0 0% 94%)',
    muted: 'hsl(210 14% 20%)',
    mutedForeground: 'hsl(210 10% 65%)',
    accent: 'hsl(199 100% 52%)',
    accentForeground: 'hsl(210 14% 8%)',
    destructive: 'hsl(0 65% 58%)',
    destructiveForeground: 'hsl(0 0% 100%)',
    border: 'hsl(210 14% 24%)',
    input: 'hsl(210 14% 24%)',
    ring: 'hsl(199 100% 52%)',
    radius: '0.625rem',
  },
};
