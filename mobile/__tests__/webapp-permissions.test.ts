import { humanizePermission } from '../lib/webapp-permissions';

describe('humanizePermission', () => {
  it('describes the notification permission without naming one phone platform', () => {
    expect(humanizePermission('notifications').title).toBe(
      'show phone notifications',
    );
  });

  it('falls back to the raw permission string', () => {
    const copy = humanizePermission('something.new');
    expect(copy.title).toBe('something.new');
    expect(copy.subtitle).toBeUndefined();
  });

  it('treats audio and audio.tts as the same capability', () => {
    expect(humanizePermission('audio')).toEqual(
      humanizePermission('audio.tts'),
    );
  });
});
