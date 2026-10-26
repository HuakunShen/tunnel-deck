/**
 * Build for the Solid TUI: one self-contained Node ESM file. `univiewSolid()`
 * applies Solid's universal-renderer transform (plain esbuild cannot), and only
 * `node:` builtins stay external so the bundle runs without `node_modules`.
 */
import { resolve } from "node:path";
import { univiewSolid } from "@uniview/tui-solid/vite";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [univiewSolid()],
  ssr: { noExternal: true },
  test: { environment: "node", include: ["tests/**/*.test.{ts,tsx}"] },
  build: {
    ssr: resolve(import.meta.dirname, "src/main.tsx"),
    target: "esnext",
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      external: (id: string): boolean => id.startsWith("node:"),
      output: {
        format: "es",
        banner: "#!/usr/bin/env node",
        entryFileNames: "[name].js",
        chunkFileNames: "chunks/[name]-[hash].js",
      },
    },
  },
});
