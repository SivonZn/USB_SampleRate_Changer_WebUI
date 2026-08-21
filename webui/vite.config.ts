import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

const outputDirectory = process.env.WEBUI_OUT_DIR ?? "../webroot";

export default defineConfig({
  base: "./",
  build: {
    outDir: outputDirectory,
    emptyOutDir: true,
    target: "esnext"
  },
  plugins: [solid()]
});
