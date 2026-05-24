import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';

export default [
  eslint.configs.recommended,
  ...tseslint.configs.recommendedTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        project: true,
      },
    },
    rules: {
      // TypeScript checks undefined identifiers more precisely than the
      // base `no-undef` rule, which has no knowledge of DOM / Node
      // globals or per-file lib config.
      'no-undef': 'off',
      '@typescript-eslint/no-unused-vars': [
        'warn',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],
    },
  },
  {
    // Codegen merges `interface BridgethingClient extends ClientSurfaces {}`
    // onto the class so consumers see typed surface accessors. Empty-extending
    // interfaces and class+interface merging are precisely the pattern this
    // file is built on; the lint defaults exist to catch accidental uses.
    files: ['src/index.ts'],
    rules: {
      '@typescript-eslint/no-empty-object-type': 'off',
      '@typescript-eslint/no-unsafe-declaration-merging': 'off',
    },
  },
  {
    files: ['**/*.js'],
    ...tseslint.configs.disableTypeChecked,
  },
  {
    ignores: ['dist/**', 'tests/**', 'src/env.d.ts', 'README.md'],
  },
];
