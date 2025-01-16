import type { Options } from 'tsup';

export const tsup: Options = {
  target: 'es2020',
  format: ['cjs', 'esm'],
  splitting: false,
  sourcemap: true,
  clean: true,
  dts: true,
};
