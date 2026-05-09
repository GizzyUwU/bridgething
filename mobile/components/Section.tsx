import type { ReactNode } from 'react';
import { Text, View } from 'react-native';

/** Vertical group with an uppercase label header. The brand UI uses these
 *  on every list page; centralising spacing keeps headers aligned. */
export function Section({
  title,
  children,
  className,
}: {
  title?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <View className={`mb-5 ${className ?? ''}`}>
      {title ? (
        <Text className="mb-2 text-[11px] font-semibold uppercase tracking-widest text-muted-foreground">
          {title}
        </Text>
      ) : null}
      {children}
    </View>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return (
    <Text className="text-xs italic text-muted-foreground">{children}</Text>
  );
}
