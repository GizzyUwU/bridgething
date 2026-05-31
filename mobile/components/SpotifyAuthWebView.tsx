import { StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { WebView, type WebViewNavigation } from 'react-native-webview';

import { usePkceWebView } from '../lib/spotify-auth';
import { Press } from './Press';

export function SpotifyAuthWebView() {
  const request = usePkceWebView(s => s.request);
  if (!request) return null;

  const intercept = (url: string): boolean => {
    if (!url.startsWith(request.redirectPrefix)) return false;
    request.onCallback(url);
    return true;
  };

  return (
    <View style={StyleSheet.absoluteFill} className="z-50 bg-background">
      <SafeAreaView edges={['top', 'bottom']} className="flex-1 bg-background">
        <View className="flex-row items-center justify-between px-5 py-3">
          <Text className="text-[16px] font-semibold text-foreground">
            sign in to Spotify
          </Text>
          <Press onPress={request.onCancel} scaleTo={0.94}>
            <Text className="text-[15px] font-semibold text-primary">
              cancel
            </Text>
          </Press>
        </View>
        <WebView
          source={{ uri: request.url }}
          onShouldStartLoadWithRequest={(req: WebViewNavigation) =>
            !intercept(req.url)
          }
          onNavigationStateChange={(nav: WebViewNavigation) => {
            intercept(nav.url);
          }}
        />
      </SafeAreaView>
    </View>
  );
}
