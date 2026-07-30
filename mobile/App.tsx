import './global.css';

import {
  DarkTheme as NavDarkTheme,
  DefaultTheme as NavLightTheme,
  NavigationContainer,
  type Theme as NavTheme,
} from '@react-navigation/native';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import { PortalHost } from '@rn-primitives/portal';
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
import { refreshCatalog } from './lib/catalog';
import { bootstrapSession } from './lib/session';
import { getSetupCompleted } from './lib/storage';
import { PALETTE } from './lib/theme';
import type { RootStackParamList } from './navigation';
import { DashboardScreen } from './screens/Dashboard';
import { DebugScreen } from './screens/Debug';
import { LogsScreen } from './screens/Logs';
import { OtaVersionsScreen } from './screens/OtaVersions';
import { SettingsScreen } from './screens/Settings';
import { SetupScreen } from './screens/Setup';
import { StoreScreen } from './screens/Store';
import { StoreAppScreen } from './screens/StoreApp';
import { StoreSourceScreen } from './screens/StoreSource';
import { WebappBrowseScreen } from './screens/WebappBrowse';
import { WebappDetailScreen } from './screens/WebappDetail';
import { WebappSettingsScreen } from './screens/WebappSettings';
import { WebappSlotsScreen } from './screens/WebappSlots';

const Stack = createNativeStackNavigator<RootStackParamList>();

type BootRoute = 'Dashboard' | 'Setup';

export default function App() {
  const scheme = useColorScheme() ?? 'light';
  const isDark = scheme === 'dark';
  const navTheme = makeNavTheme(isDark);
  const palette = isDark ? PALETTE.dark : PALETTE.light;

  const [boot, setBoot] = useState<BootRoute | null>(null);

  useEffect(() => {
    let cancelled = false;
    setBoot(getSetupCompleted() ? 'Dashboard' : 'Setup');
    bootstrapSession().catch(err => {
      if (cancelled) return;
      console.warn('[bridgething] bootstrap failed', err);
    });
    refreshCatalog().catch(err => {
      if (cancelled) return;
      console.warn('[bridgething] initial catalog fetch failed', err);
    });
    return () => {
      cancelled = true;
    };
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
            name="Store"
            component={StoreScreen}
            options={{ title: 'app store', headerLargeTitle: false }}
          />
          <Stack.Screen
            name="StoreSource"
            component={StoreSourceScreen}
            options={{ title: 'source', headerLargeTitle: false }}
          />
          <Stack.Screen
            name="StoreApp"
            component={StoreAppScreen}
            options={{ title: 'app', headerLargeTitle: false }}
          />
          <Stack.Screen
            name="WebappDetail"
            component={WebappDetailScreen}
            options={{ title: 'app', headerLargeTitle: false }}
          />
          <Stack.Screen
            name="WebappSettings"
            component={WebappSettingsScreen}
            options={{ headerRight: undefined, headerLargeTitle: false }}
          />
          <Stack.Screen
            name="WebappSlots"
            component={WebappSlotsScreen}
            options={{ title: 'home screen', headerLargeTitle: false }}
          />
          <Stack.Screen
            name="Settings"
            component={SettingsScreen}
            options={{ title: 'settings', headerRight: undefined }}
          />
          <Stack.Screen
            name="OtaVersions"
            component={OtaVersionsScreen}
            options={{
              title: 'choose version',
              headerRight: undefined,
              ...(Platform.OS === 'ios' ? { headerLargeTitle: false } : {}),
            }}
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
          <Stack.Screen
            name="Debug"
            component={DebugScreen}
            options={{
              title: 'debug',
              headerRight: undefined,
              ...(Platform.OS === 'ios' ? { headerLargeTitle: false } : {}),
            }}
          />
        </Stack.Navigator>
      </NavigationContainer>
      <PortalHost />
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
