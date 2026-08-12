import * as astro from 'prettier-plugin-astro';
import * as organizeImports from 'prettier-plugin-organize-imports';
import * as tailwind from 'prettier-plugin-tailwindcss';

/** @type {import("prettier").Config} */
export default {
  bracketSpacing: true,
  bracketSameLine: true,
  singleQuote: true,
  trailingComma: 'all',
  arrowParens: 'avoid',
  semi: true,
  plugins: [organizeImports, astro, tailwind],
  overrides: [
    {
      files: '*.astro',
      options: {
        parser: 'astro',
      },
    },
    {
      files: ['*.ts', '*.js', '*.tsx', '*.jsx', '*.cjs', '*.mjs', '*.astro'],
      options: {
        printWidth: 120,
      },
    },
    {
      files: ['*.html'],
      options: {
        printWidth: 100,
      },
    },
  ],
};
