import { defineConfig } from 'vitest/config';
import {readFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import react from '@vitejs/plugin-react';
export default defineConfig({
  plugins: [react(),{name:'redesign-fixtures',apply:'serve',configureServer(server){server.middlewares.use('/__fixtures/redesign/',(request,response,next)=>{const name=(request.url??'').split('?')[0].replace(/^\//,'');if(!/^(photo|video)-[1-6]\.jpg$/.test(name)){response.statusCode=404;response.end();return;}void readFile(resolve('tests/fixtures/redesign',name)).then(bytes=>{response.setHeader('Content-Type','image/jpeg');response.end(bytes);}).catch(next);});}}],
  clearScreen: false,
  server: {
    port: 1420, strictPort: true, host: '127.0.0.1',
    watch: { ignored: ['**/.tools/**', '**/.superpowers/**', '**/src-tauri/target/**', '**/releases/**'] },
  },
  test: { environment: 'jsdom', include: ['tests/unit/**/*.test.{ts,tsx}'], setupFiles: ['tests/unit/setup.ts'] },
});
