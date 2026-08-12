import { describe, expect, it } from 'bun:test';
import { APP_DETAIL_SHELL, appDetailPath, appIdFromPath } from './app-routes.ts';

const CALENDAR_ID = '019e6701-13f8-71b5-ba04-85d326630e98';

describe('appIdFromPath', () => {
  it('reads the id out of an app detail path', () => {
    expect(appIdFromPath(`/apps/${CALENDAR_ID}`)).toBe(CALENDAR_ID);
    expect(appIdFromPath(`/apps/${CALENDAR_ID}/`)).toBe(CALENDAR_ID);
    expect(appIdFromPath(`/apps/${CALENDAR_ID.toUpperCase()}`)).toBe(CALENDAR_ID);
  });

  it('ignores everything that is not an app detail path', () => {
    expect(appIdFromPath('/apps')).toBeNull();
    expect(appIdFromPath('/apps/')).toBeNull();
    expect(appIdFromPath(APP_DETAIL_SHELL)).toBeNull();
    expect(appIdFromPath('/apps/not-a-uuid')).toBeNull();
    expect(appIdFromPath(`/apps/${CALENDAR_ID}/versions`)).toBeNull();
    expect(appIdFromPath(`/docs/${CALENDAR_ID}`)).toBeNull();
  });
});

describe('appDetailPath', () => {
  it('round-trips with appIdFromPath', () => {
    expect(appIdFromPath(appDetailPath(CALENDAR_ID))).toBe(CALENDAR_ID);
  });
});
