import type {
  BridgethingConfigField,
  SessionEvent,
} from '@bridgething/session-react-native';
import type { NativeStackScreenProps } from '@react-navigation/native-stack';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { WebView, type WebViewMessageEvent } from 'react-native-webview';
import type { ShouldStartLoadRequest } from 'react-native-webview/lib/WebViewTypes';

import { Button } from '../components/Button';
import { getSession } from '../lib/session';
import { useWebapps } from '../lib/webapps';
import type { RootStackParamList } from '../navigation';

type Props = NativeStackScreenProps<RootStackParamList, 'WebappSettings'>;

type BridgeRequest = {
  id: number;
  verb: string;
  payload?: Record<string, unknown>;
};

export function WebappSettingsScreen({ navigation, route }: Props) {
  const session = getSession();
  const { deviceId, id, name } = route.params;
  const { list } = useWebapps(deviceId);
  const info = list.find(w => w.id === id) ?? null;

  const webviewRef = useRef<WebView<object>>(null);
  const [pageUri, setPageUri] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    navigation.setOptions({ title: `${name} settings` });
  }, [name, navigation]);

  const loadPage = useCallback(async () => {
    setError(null);
    setPageUri(null);
    try {
      const uri = await session.webappSettingsPage(deviceId, id);
      setPageUri(uri);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [deviceId, id, session]);

  useEffect(() => {
    loadPage();
  }, [loadPage]);

  const deliver = useCallback((payload: unknown) => {
    const json = JSON.stringify(JSON.stringify(payload));
    webviewRef.current?.injectJavaScript(
      `window.__bridgethingSettingsDeliver && window.__bridgethingSettingsDeliver(${json}); true;`,
    );
  }, []);

  useEffect(() => {
    const unsubscribe = session.subscribe((event: SessionEvent) => {
      if (
        event.type === 'webappDocChanged' &&
        event.deviceId === deviceId &&
        event.webappId === id
      ) {
        deliver({ event: 'docChanged', key: event.key, value: event.value });
      }
    });
    return unsubscribe;
  }, [deliver, deviceId, id, session]);

  const handleVerb = useCallback(
    async (
      verb: string,
      payload: Record<string, unknown>,
    ): Promise<unknown> => {
      const key = typeof payload.key === 'string' ? payload.key : '';
      const value = typeof payload.value === 'string' ? payload.value : '';
      switch (verb) {
        case 'context':
          return {
            webappId: id,
            name: info?.name ?? name,
            version: info?.version ?? '',
            deviceId,
          };
        case 'config.fields':
          return (info?.config ?? []).map(toWireConfigField);
        case 'config.list':
          return session.listWebappConfig(deviceId, id);
        case 'config.set':
          await session.setWebappConfigField(deviceId, id, key, value);
          return { key, value };
        case 'config.delete': {
          await session.deleteWebappConfigField(deviceId, id, key);
          const fresh = await session.listWebappConfig(deviceId, id);
          return { key, value: fresh.find(e => e.key === key)?.value ?? null };
        }
        case 'doc.get':
          return { key, value: await session.getWebappDoc(deviceId, id, key) };
        case 'doc.list':
          return session.listWebappDoc(deviceId, id);
        case 'doc.set':
          await session.setWebappDoc(deviceId, id, key, value);
          return { key, value };
        case 'doc.delete':
          await session.deleteWebappDoc(deviceId, id, key);
          return { key, value: null };
        default:
          throw new Error(`unknown settings bridge verb: ${verb}`);
      }
    },
    [deviceId, id, info, name, session],
  );

  const onMessage = useCallback(
    (event: WebViewMessageEvent) => {
      let request: BridgeRequest;
      try {
        request = JSON.parse(event.nativeEvent.data) as BridgeRequest;
      } catch {
        return;
      }
      if (typeof request.id !== 'number' || typeof request.verb !== 'string')
        return;
      if (request.verb === 'done') {
        navigation.goBack();
        return;
      }
      void (async () => {
        try {
          const value = await handleVerb(request.verb, request.payload ?? {});
          deliver({ id: request.id, ok: true, value });
        } catch (err) {
          deliver({
            id: request.id,
            ok: false,
            error: err instanceof Error ? err.message : String(err),
          });
        }
      })();
    },
    [deliver, handleVerb, navigation],
  );

  if (error) {
    return (
      <View className="flex-1 items-center justify-center gap-4 bg-background px-6">
        <Text className="text-center text-[14px] text-destructive">
          {error}
        </Text>
        <Button onPress={loadPage} variant="primary">
          retry
        </Button>
      </View>
    );
  }

  if (!pageUri) {
    return (
      <View className="flex-1 items-center justify-center gap-3 bg-background">
        <ActivityIndicator size="small" color="hsl(199 100% 44%)" />
        <Text className="text-[13px] text-muted-foreground">
          fetching settings from your car thing...
        </Text>
      </View>
    );
  }

  return (
    <WebView<object>
      ref={webviewRef}
      source={{ uri: pageUri }}
      originWhitelist={['file://*']}
      allowFileAccess
      onMessage={onMessage}
      onShouldStartLoadWithRequest={(req: ShouldStartLoadRequest) =>
        req.url === pageUri || req.url.startsWith('file://')
      }
      style={{ flex: 1 }}
    />
  );
}

function toWireConfigField(
  field: BridgethingConfigField,
): Record<string, unknown> {
  switch (field.kind) {
    case 'number':
      return {
        type: 'number',
        data: {
          key: field.key,
          label: field.label,
          min: field.min,
          max: field.max,
          step: field.step,
          default:
            field.defaultValue !== undefined
              ? Number(field.defaultValue)
              : undefined,
        },
      };
    case 'boolean':
      return {
        type: 'boolean',
        data: {
          key: field.key,
          label: field.label,
          default:
            field.defaultValue !== undefined
              ? field.defaultValue === 'true'
              : undefined,
        },
      };
    case 'enum':
      return {
        type: 'enum',
        data: {
          key: field.key,
          label: field.label,
          choices: field.choices ?? [],
          default: field.defaultValue,
        },
      };
    case 'secret':
    case 'string':
    default:
      return {
        type: field.kind === 'secret' ? 'secret' : 'string',
        data: {
          key: field.key,
          label: field.label,
          pattern: field.pattern,
          minLength: field.minLength,
          maxLength: field.maxLength,
          default: field.defaultValue,
        },
      };
  }
}
