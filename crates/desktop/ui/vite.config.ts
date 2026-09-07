import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri serves this on a fixed port in development and from `dist/` in a bundle.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // The Rust side has its own rebuild; watching it here just churns.
      ignored: ['**/src-tauri/**', '**/target/**'],
    },
  },
  build: {
    // Matches the webview Tauri ships on the platforms in scope.
    target: 'es2021',
    sourcemap: true,
  },
});
