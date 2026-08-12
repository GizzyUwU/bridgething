import type { InstalledWebapp } from '@bridgething/catalog';
import type { WebappInfo } from '@bridgething/companion-types';

export function toInstalled(webapps: WebappInfo[]): InstalledWebapp[] {
  return webapps.map(info => ({
    id: info.id,
    version: info.version,
    source: info.source,
    role: info.role,
    provenance: info.provenance,
  }));
}
