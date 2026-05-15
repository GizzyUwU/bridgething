import './global.css';

import {
  DarkTheme as NavDarkTheme,
  DefaultTheme as NavLightTheme,
  NavigationContainer,
  type Theme as NavTheme,
} from '@react-navigation/native';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import { Settings as SettingsIcon } from 'lucide-react-native';
import { useEffect, useState } from 'react';
import {
  ActivityIndicator,
  Platform,
  StatusBar,
  useColorScheme,
  View,
} from 'react-native';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import { Press } from './components/Press';
import { Wordmark } from './components/Wordmark';
import {
  dismissVerificationBrowser,
  openVerificationBrowser,
} from './lib/auth-browser';
import { bootstrapSession, getSession } from './lib/session';
import { getSetupCompleted } from './lib/storage';
import { PALETTE } from './lib/theme';
import type { RootStackParamList } from './navigation';
import { DashboardScreen } from './screens/Dashboard';
import { LogsScreen } from './screens/Logs';
import { SettingsScreen } from './screens/Settings';
import { SetupScreen } from './screens/Setup';
import { WebappBrowseScreen } from './screens/WebappBrowse';
import { WebappDetailScreen } from './screens/WebappDetail';

const Stack = createNativeStackNavigator<RootStackParamList>();

type BootRoute = 'Dashboard' | 'Setup';

export default function App() {
  const scheme = useColorScheme() ?? 'light';
  const isDark = scheme === 'dark';
  const navTheme = makeNavTheme(isDark);
  const palette = isDark ? PALETTE.dark : PALETTE.light;

  const [boot, setBoot] = useState<BootRoute | null>(null);

  // Pick the initial route from the mmkv-stored "setup completed"
  // flag, then start the session in the background. New users land on
  // Setup; everyone else hits Dashboard with the status strip nudging
  // them toward whatever's still missing. Bootstrap failures are
  // logged but don't block the route — we'd rather render Setup with
  // an in-screen error than stick on splash.
  useEffect(() => {
    let cancelled = false;
    setBoot(getSetupCompleted() ? 'Dashboard' : 'Setup');
    bootstrapSession().catch(err => {
      if (cancelled) return;
      console.warn('[bridgething] bootstrap failed', err);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // Drive the OAuth in-app browser off the auth-state event stream.
  // When the glue lands on `pending` with a complete verification URL,
  // open the browser; close it once auth resolves.
  useEffect(() => {
    const session = getSession();
    return session.subscribe(event => {
      if (event.type !== 'authStateChanged') return;
      if (
        event.state.kind === 'pending' &&
        event.state.verificationUrlComplete
      ) {
        openVerificationBrowser(event.state.verificationUrlComplete).catch(
          err => {
            console.warn('[bridgething] verification browser open failed', err);
          },
        );
      } else {
        // authenticated / failed / idle all want the browser gone.
        dismissVerificationBrowser().catch(() => {});
      }
    });
  }, []);

  if (boot == null) {
    return (
      <SafeAreaProvider>
        <StatusBar
          barStyle={isDark ? 'light-content' : 'dark-content'}
          backgroundColor={palette.background}
        />
        <View
          className="flex-1 items-center justify-center"
          style={{ backgroundColor: palette.background }}
        >
          <Wordmark size="lg" />
          <View className="mt-8">
            <ActivityIndicator size="small" color={palette.primary} />
          </View>
        </View>
      </SafeAreaProvider>
    );
  }

  return (
    <SafeAreaProvider>
      <StatusBar
        barStyle={isDark ? 'light-content' : 'dark-content'}
        backgroundColor={palette.background}
      />
      <NavigationContainer theme={navTheme}>
        <Stack.Navigator
          initialRouteName={boot}
          screenOptions={({ navigation }) => ({
            headerStyle: { backgroundColor: palette.background },
            headerTintColor: palette.foreground,
            headerShadowVisible: false,
            headerTitleStyle: {
              fontWeight: '700',
              fontSize: 17,
              letterSpacing: -0.2,
            },
            headerLargeTitleStyle: {
              fontWeight: '800',
              letterSpacing: -0.6,
            },
            contentStyle: { backgroundColor: palette.background },
            headerBackButtonDisplayMode: 'minimal',
            headerRight: () => (
              <Press
                onPress={() => navigation.navigate('Settings')}
                hitSlop={10}
                scaleTo={0.9}
                style={{
                  paddingHorizontal: 6,
                  paddingVertical: 6,
                  borderRadius: 999,
                }}
              >
                <SettingsIcon
                  size={20}
                  color={palette.foreground}
                  strokeWidth={2.2}
                />
              </Press>
            ),
          })}
        >
          <Stack.Screen
            name="Setup"
            component={SetupScreen}
            options={{ headerShown: false }}
          />
          <Stack.Screen
            name="Dashboard"
            component={DashboardScreen}
            options={{
              headerTitle: () => <Wordmark size="sm" />,
            }}
          />
          <Stack.Screen
            name="WebappBrowse"
            component={WebappBrowseScreen}
            options={{ title: 'apps', headerLargeTitle: false }}
          />
          <Stack.Screen
            name="WebappDetail"
            component={WebappDetailScreen}
            options={{ title: 'app', headerLargeTitle: false }}
          />
          <Stack.Screen
            name="Settings"
            component={SettingsScreen}
            options={{ title: 'settings', headerRight: undefined }}
          />
          <Stack.Screen
            name="Logs"
            component={LogsScreen}
            options={{
              title: 'logs',
              headerRight: undefined,
              ...(Platform.OS === 'ios' ? { headerLargeTitle: false } : {}),
            }}
          />
        </Stack.Navigator>
      </NavigationContainer>
    </SafeAreaProvider>
  );
}

function makeNavTheme(dark: boolean): NavTheme {
  const palette = dark ? PALETTE.dark : PALETTE.light;
  const base = dark ? NavDarkTheme : NavLightTheme;
  return {
    ...base,
    dark,
    colors: {
      ...base.colors,
      background: palette.background,
      card: palette.background,
      text: palette.foreground,
      border: palette.border,
      primary: palette.primary,
      notification: palette.destructive,
    },
  };
}
