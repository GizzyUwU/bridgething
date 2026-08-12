import { View } from 'react-native';

import { Icon, type IconName } from './Icon';
import { BOX, type BoxSize, type Tone } from '../lib/theme';
import { TONE_BG } from '../lib/tone';

export function IconBadge({
  name,
  tone = 'accent',
  size = 'md',
}: {
  name: IconName;
  tone?: Tone;
  size?: BoxSize;
}) {
  const box = BOX[size];
  return (
    <View
      className={`items-center justify-center ${TONE_BG[tone]}`}
      style={{ width: box, height: box }}
    >
      <Icon name={name} tone={tone} size={Math.round(box * 0.5)} />
    </View>
  );
}
