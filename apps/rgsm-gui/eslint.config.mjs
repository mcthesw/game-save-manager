import js from '@eslint/js';
import vue from 'eslint-plugin-vue';
import vueTsEslint from '@vue/eslint-config-typescript';
import eslintConfigPrettier from 'eslint-config-prettier';
import tseslint from 'typescript-eslint';

export default [
  {
    ignores: [
      '**/node_modules/**',
      'e2e-report/**',
      'e2e-results/**',
      '.nuxt/**',
      '.output/**',
      '.data/**',
      'dist/**',
      'src-tauri/**',
      'scripts/**',
      'types/**',
      'src/public/**',
      'src/bindings.ts',
      'auto-imports.d.ts',
      'components.d.ts',
      'typed-router.d.ts',
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...vue.configs['flat/recommended'],
  ...vueTsEslint(),
  eslintConfigPrettier,
  {
    rules: {
      'no-undef': 'off',
      'vue/multi-word-component-names': 'off',
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
      // Element Plus 已全面移除,禁止回流(S8);新 UI 一律走 src/ui/kit
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['element-plus', 'element-plus/*', '@element-plus/*'],
              message: 'Element Plus was removed (S8). Use src/ui/kit components instead.',
            },
          ],
        },
      ],
    },
  },
];
