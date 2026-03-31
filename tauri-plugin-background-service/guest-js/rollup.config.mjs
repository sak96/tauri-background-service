import typescript from '@rollup/plugin-typescript';

export default {
  input: 'index.ts',
  output: [
    {
      file: 'dist-js/index.js',
      format: 'esm',
      sourcemap: true,
    },
    {
      file: 'dist-js/index.cjs',
      format: 'cjs',
      sourcemap: true,
    },
  ],
  external: /^@tauri-apps\/api/,
  plugins: [typescript()],
};
