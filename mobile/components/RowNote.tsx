import { View } from 'react-native';

import { Note } from './Note';
import type { Tone } from '../lib/theme';

export type RowNotice = {
  tone?: Tone;
  text: string;
  action?: string;
  onAction?: () => void;
};

export function RowNote({ notice }: { notice: RowNotice | null }) {
  if (!notice) return null;
  return (
    <View className="px-4 pb-3">
      <Note
        tone={notice.tone ?? 'err'}
        action={notice.action}
        onAction={notice.onAction}
      >
        {notice.text}
      </Note>
    </View>
  );
}
