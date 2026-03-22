import withNuxt from './.nuxt/eslint.config.mjs';
import eslintConfigPrettier from 'eslint-config-prettier';

export default withNuxt({
  ignores: [
    '**/node_modules/**',
    '.nuxt/**',
    '.output/**',
    'dist/**',
    'src-tauri/**',
    'scripts/**',
    'types/**',
    'src/public/**',
    'src/bindings.ts',
  ],
}).append(eslintConfigPrettier);
