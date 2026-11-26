import { defineConfig, type Plugin } from 'vite';
import react from '@vitejs/plugin-react';
import { mockApiPlugin } from './src/mockApi';

const useMock = process.env.VITE_USE_MOCK === 'true';
const apiTarget = process.env.VITE_API_TARGET || 'http://127.0.0.1:8080';

function probeBlockerPlugin(): Plugin {
  return {
    name: 'chess-hustle-probe-blocker',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const pathname = (req.url || '').split('?')[0] || '';
        if (
          pathname.startsWith('/.') ||
          pathname.includes('/.') ||
          pathname.includes('..') ||
          pathname.includes('etc/passwd') ||
          pathname.includes('proc/self') ||
          pathname.endsWith('.env') ||
          pathname.endsWith('.php') ||
          pathname.endsWith('.git')
        ) {
          res.statusCode = 404;
          res.setHeader('Content-Type', 'text/plain');
          return res.end('Not Found');
        }
        next();
      });
    },
  };
}

export default defineConfig({
  plugins: [react(), probeBlockerPlugin(), ...(useMock ? [mockApiPlugin()] : [])],
  server: {
    host: '0.0.0.0',
    port: 5173,
    proxy: useMock
      ? undefined
      : {
          '/v1': {
            target: apiTarget,
            changeOrigin: true,
          },
          '/ws': {
            target: apiTarget,
            ws: true,
            changeOrigin: true,
          },
          '/healthz': {
            target: apiTarget,
            changeOrigin: true,
          },
          '/readyz': {
            target: apiTarget,
            changeOrigin: true,
          },
        },
  },
});
