import { useEffect, useState } from 'react';

import { Button, type ButtonSize } from './Button';
import type { IconName } from './Icon';

const ARM_MS = 4000;

export function ArmedButton({
  label,
  confirmLabel,
  onConfirm,
  tone = 'err',
  size = 'md',
  icon,
  full = true,
  disabled,
}: {
  label: string;
  confirmLabel: string;
  onConfirm: () => void;
  tone?: 'neutral' | 'err';
  size?: ButtonSize;
  icon?: IconName;
  full?: boolean;
  disabled?: boolean;
}) {
  const [armed, setArmed] = useState(false);

  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), ARM_MS);
    return () => clearTimeout(timer);
  }, [armed]);

  return (
    <Button
      variant={
        armed ? (tone === 'err' ? 'destructive' : 'primary') : 'secondary'
      }
      size={size}
      icon={icon}
      full={full}
      disabled={disabled}
      onPress={() => {
        if (!armed) {
          setArmed(true);
          return;
        }
        setArmed(false);
        onConfirm();
      }}
    >
      {armed ? confirmLabel : label}
    </Button>
  );
}
