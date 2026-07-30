import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';

const TIMER_MESSAGE =
  'use usePoll from lib/poll instead. RN re-arms JS timers off an NSTimer once it parks the display link, so a bare interval keeps polling while the app is backgrounded.';

const FRAME_MESSAGE =
  'requestAnimationFrame drives the worklets display link, which keeps running while the app is backgrounded. gate it on useAppActive from lib/app-active.';

export default [
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    languageOptions: {
      parserOptions: {
        ecmaFeatures: {
          jsx: true,
        },
      },
    },
    rules: {
      'no-undef': 'off',
      '@typescript-eslint/no-unused-vars': [
        'warn',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],
      'no-restricted-globals': [
        'error',
        { name: 'setInterval', message: TIMER_MESSAGE },
        { name: 'requestAnimationFrame', message: FRAME_MESSAGE },
      ],
    },
    settings: {
      react: {
        version: 'detect',
      },
    },
  },
  {
    files: ['lib/poll.ts'],
    rules: {
      'no-restricted-globals': 'off',
    },
  },
  {
    files: ['*.config.js', '*.config.cjs', '__tests__/**', '__mocks__/**'],
    rules: {
      '@typescript-eslint/no-require-imports': 'off',
    },
  },
  {
    ignores: ['ios', 'android'],
  },
];
