import js from '@eslint/js';
import ts from 'typescript-eslint';
export default ts.config(
  { ignores: ['src/app/contracts.ts', 'dist/**'] },
  js.configs.recommended,
  ...ts.configs.recommended,
  { files: ['**/*.{js,mjs,ts,tsx}'], languageOptions: { globals: { console: 'readonly', process: 'readonly', URL: 'readonly', document: 'readonly', window: 'readonly', localStorage: 'readonly', requestAnimationFrame: 'readonly', setTimeout: 'readonly', clearTimeout: 'readonly' } } },
);
