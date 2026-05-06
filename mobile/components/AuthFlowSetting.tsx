import { Pressable, Text, View } from 'react-native';

export type AuthFlow = 'webview-pkce' | 'device-code';

export type AuthFlowSettingProps = {
  configured: { webviewPKCE: true; deviceCode: boolean };
  selected: AuthFlow;
  onChange: (flow: AuthFlow) => void;
};

export function AuthFlowSetting({
  configured,
  selected,
  onChange,
}: AuthFlowSettingProps) {
  if (!configured.deviceCode) return null;

  return (
    <View className="mb-5">
      <Text className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
        spotify auth flow
      </Text>
      <Pressable
        onPress={() => onChange('device-code')}
        className={`mb-1.5 rounded-md px-3 py-2 ${selected === 'device-code' ? 'bg-primary' : 'bg-secondary'}`}
      >
        <Text className="text-sm font-semibold">
          device code (system browser)
        </Text>
      </Pressable>
      <Pressable
        onPress={() => onChange('webview-pkce')}
        className={`mb-1.5 rounded-md px-3 py-2 ${selected === 'webview-pkce' ? 'bg-primary' : 'bg-secondary'}`}
      >
        <Text className="text-sm font-semibold">webview pkce (in-app)</Text>
      </Pressable>
    </View>
  );
}
