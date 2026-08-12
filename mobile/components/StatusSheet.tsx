import { describeError } from '@bridgething/ui/errors';
import { useState } from 'react';
import { Text, View } from 'react-native';

import { Button } from './Button';
import { ConditionList } from './ConditionList';
import { Note } from './Note';
import { PairNote } from './PairNote';
import { Sheet } from './Sheet';
import { reconcileAll } from '../lib/bridge';
import { type Condition, useConditionAction } from '../lib/status';
import { TEXT } from '../lib/theme';

export function StatusSheet({
  visible,
  conditions,
  onClose,
}: {
  visible: boolean;
  conditions: Condition[];
  onClose: () => void;
}) {
  const { run, busy, notice } = useConditionAction();
  const [resyncing, setResyncing] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const resync = async () => {
    setResyncing(true);
    setFailure(null);
    try {
      await reconcileAll();
    } catch (err) {
      setFailure(describeError(err));
    } finally {
      setResyncing(false);
    }
  };

  return (
    <Sheet visible={visible} onClose={onClose}>
      <Text className="font-mono uppercase text-accent" style={TEXT.eyebrow}>
        {conditions.length === 0
          ? 'all clear'
          : conditions.length === 1
            ? '1 issue'
            : `${conditions.length} issues`}
      </Text>

      {conditions.length === 0 ? (
        <Text className="font-sans text-muted" style={TEXT.hint}>
          nothing needs your attention.
        </Text>
      ) : (
        <ConditionList
          conditions={conditions}
          action={condition => {
            const action = condition.action;
            if (!action) return null;
            return (
              <View className="mt-1 flex-row">
                <Button
                  variant="secondary"
                  size="md"
                  full={false}
                  loading={action.kind === 'pair' && busy}
                  onPress={() => {
                    run(action);
                    if (action.kind !== 'pair') onClose();
                  }}
                >
                  {action.label}
                </Button>
              </View>
            );
          }}
        />
      )}

      <PairNote notice={notice} />

      {failure ? <Note tone="err">{failure}</Note> : null}

      <View className="flex-row justify-between gap-2">
        <Button
          variant="secondary"
          size="md"
          full={false}
          icon="RefreshCw"
          loading={resyncing}
          onPress={resync}
        >
          resync everything
        </Button>
        <Button variant="ghost" size="md" full={false} onPress={onClose}>
          close
        </Button>
      </View>
    </Sheet>
  );
}
