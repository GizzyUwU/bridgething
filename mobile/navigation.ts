export type RootStackParamList = {
  Setup: { step?: number } | undefined;
  Dashboard: undefined;
  WebappBrowse: { deviceId: string };
  Store: { deviceId: string } | undefined;
  StoreSource: { deviceId: string | null; url: string; name: string };
  StoreApp: { deviceId: string | null; appId: string; sourceUrl: string };
  WebappDetail: { deviceId: string; id: string };
  WebappSettings: { deviceId: string; id: string; name: string };
  WebappSlots: { deviceId: string };
  Settings: undefined;
  OtaVersions: { deviceId: string; channel: string };
  Logs: undefined;
  Debug: undefined;
};
