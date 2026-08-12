import { useCallback, useRef, useState } from 'preact/hooks';

export type LogKind = 'info' | 'ok' | 'warn' | 'err';
export type LogLine = { id: number; at: number; kind: LogKind; message: string };
export type Say = (message: string, kind?: LogKind) => void;

const MAX_LINES = 200;

export function useConsoleLog(): { lines: LogLine[]; say: Say } {
  const [lines, setLines] = useState<LogLine[]>([]);
  const seq = useRef(0);

  const say = useCallback<Say>((message, kind = 'info') => {
    setLines(prev => [...prev, { id: seq.current++, at: performance.now(), kind, message }].slice(-MAX_LINES));
  }, []);

  return { lines, say };
}
