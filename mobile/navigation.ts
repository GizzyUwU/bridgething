/**
 * Centralised stack route map. Screens import the param-list to type
 * their `navigation` props. Adding a new screen = add a route here +
 * register it in `App.tsx`.
 *
 * Webapp routes carry the target `deviceId` so multi-device install /
 * detail flows route to the correct Car Thing.
 */
export type RootStackParamList = {
  Setup: undefined;
  Dashboard: undefined;
  WebappBrowse: { deviceId: string };
  WebappDetail: { deviceId: string; id: string };
  Settings: undefined;
};
