import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart';

import { resource } from './resource.ts';

export const autostart = resource(false, () => isEnabled());

export async function setAutostart(enabled: boolean): Promise<void> {
  if (enabled) await enable();
  else await disable();
  await autostart.refresh();
}
