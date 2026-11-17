import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      // Dev proxy to hustle-server (Phase 0: meta/config).
      '/v1': 'http://localhost:8080',
    },
  },
});
