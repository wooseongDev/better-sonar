import { defineConfig } from 'oxfmt'

export default defineConfig({
  ignorePatterns: [
    'dist/**',
    'node_modules/**',
    'src-tauri/gen/**',
    'src-tauri/target/**',
    'target/**',
    'artifacts/**',
  ],
  printWidth: 120,
  singleQuote: true,
  semi: false,
  trailingComma: 'all',
})
