import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
import { readFileSync } from "node:fs";

const outputDirectory = process.env.WEBUI_OUT_DIR ?? "../webroot";
const version = readFileSync(new URL("../VERSION", import.meta.url), "utf8").trim();

export default defineConfig({
  base: "./",
  build: {
    outDir: outputDirectory,
    emptyOutDir: true,
    target: "esnext"
  },
  define: {
    __WEBUI_VERSION__: JSON.stringify(version)
  },
  plugins: [solid()]
});
