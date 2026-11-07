import { defineConfig } from 'vite';

export default defineConfig({
  base: '/byz_cascading_quorum/',
  build: {
    outDir: '../dist',
    emptyOutDir: true
  }
});
