import './global.css';

import {
  DarkTheme as NavDarkTheme,
  DefaultTheme as NavLightTheme,
  NavigationContainer,
  type Theme as NavTheme,
} from '@react-navigation/native';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import { Pressable, StatusBar, Text, useColorScheme } from 'react-native';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import { THEME } from './lib/theme';
import type { RootStackParamList } from './navigation';
import { DashboardScreen } from './screens/Dashboard';
import { SettingsScreen } from './screens/Settings';
import { SetupScreen } from './screens/Setup';
import { WebappBrowseScreen } from './screens/WebappBrowse';
import { WebappDetailScreen } from './screens/WebappDetail';

const Stack = createNativeStackNavigator<RootStackParamList>();

export default function App() {
  const scheme = useColorScheme() ?? 'light';
  const navTheme = makeNavTheme(scheme === 'dark');

  return (
    <SafeAreaProvider>
      <StatusBar
        barStyle={scheme === 'dark' ? 'light-content' : 'dark-content'}
      />
      <NavigationContainer theme={navTheme}>
        <Stack.Navigator
          initialRouteName="Setup"
          screenOptions={({ navigation }) => ({
            headerStyle: { backgroundColor: navTheme.colors.background },
            headerTintColor: navTheme.colors.text,
            headerTitleStyle: { fontWeight: '700' },
            headerRight: () => (
              <Pressable
                onPress={() => navigation.navigate('Settings')}
                hitSlop={12}
              >
                <Text className="text-sm font-semibold text-primary">
                  settings
                </Text>
              </Pressable>
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
            options={{ title: 'bridgething' }}
          />
          <Stack.Screen
            name="WebappBrowse"
            component={WebappBrowseScreen}
            options={{ title: 'apps' }}
          />
          <Stack.Screen
            name="WebappDetail"
            component={WebappDetailScreen}
            options={{ title: 'app' }}
          />
          <Stack.Screen
            name="Settings"
            component={SettingsScreen}
            options={{ title: 'settings', headerRight: undefined }}
          />
        </Stack.Navigator>
      </NavigationContainer>
    </SafeAreaProvider>
  );
}

function makeNavTheme(dark: boolean): NavTheme {
  const palette = dark ? THEME.dark : THEME.light;
  const base = dark ? NavDarkTheme : NavLightTheme;
  return {
    ...base,
    dark,
    colors: {
      ...base.colors,
      background: palette.background,
      card: palette.card,
      text: palette.foreground,
      border: palette.border,
      primary: palette.primary,
      notification: palette.destructive,
    },
  };
}
