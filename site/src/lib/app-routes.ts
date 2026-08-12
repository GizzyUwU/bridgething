const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export const APP_DETAIL_SHELL_ID = 'lookup';
export const APP_DETAIL_SHELL = `/apps/${APP_DETAIL_SHELL_ID}/`;

export function appDetailPath(id: string): string {
  return `/apps/${id}`;
}

export function appIdFromPath(pathname: string): string | null {
  const parts = pathname.split('/').filter(Boolean);
  if (parts.length !== 2 || parts[0] !== 'apps') return null;
  const id = parts[1]!;
  return UUID.test(id) ? id.toLowerCase() : null;
}
