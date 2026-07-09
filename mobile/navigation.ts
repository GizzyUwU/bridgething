/**
 * Centralised stack route map. Screens import the param-list to type
 * their `navigation` props.
 *
 * Webapp routes carry the target `deviceId` so multi-device install /
 * detail flows route to the correct Car Thing.
 */
export type RootStackParamList = {
  Setup: { step?: number } | undefined;
  Dashboard: undefined;
  WebappBrowse: { deviceId: string };
  Store: { deviceId: string };
  WebappDetail: { deviceId: string; id: string };
  Settings: undefined;
  OtaVersions: { deviceId: string; channel: string };
  Logs: undefined;
  Debug: undefined;
};
