import { defineConfig } from 'oxlint'

export default defineConfig({
  ignorePatterns: [
    'dist/**',
    'node_modules/**',
    'src-tauri/gen/**',
    'src-tauri/target/**',
    'target/**',
    'artifacts/**',
  ],
  env: {
    browser: true,
  },
  plugins: ['react', 'typescript'],
  rules: {
    'react/rules-of-hooks': 'error',
    'react/exhaustive-deps': 'warn',
    'react/only-export-components': ['warn', { allowConstantExport: true }],
  },
})
