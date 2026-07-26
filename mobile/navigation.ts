export type RootStackParamList = {
  Setup: { step?: number } | undefined;
  Dashboard: undefined;
  WebappBrowse: { deviceId: string };
  Store: { deviceId: string } | undefined;
  WebappDetail: { deviceId: string; id: string };
  WebappSettings: { deviceId: string; id: string; name: string };
  Settings: undefined;
  OtaVersions: { deviceId: string; channel: string };
  Logs: undefined;
  Debug: undefined;
};
