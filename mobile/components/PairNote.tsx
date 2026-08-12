import { Linking } from 'react-native';

import { Note } from './Note';
import type { PairAction, PairNotice } from '../lib/session';

export function PairNote({
  notice,
  className,
}: {
  notice: PairNotice | null;
  className?: string;
}) {
  if (!notice) return null;

  const action = notice.action;
  return (
    <Note
      tone={notice.tone}
      title={notice.title}
      action={action?.label}
      onAction={action ? () => runPairAction(action) : undefined}
      className={className}
    >
      {notice.body}
    </Note>
  );
}

function runPairAction(action: PairAction): void {
  switch (action.kind) {
    case 'openSettings':
      void Linking.openSettings();
      return;
  }
}
