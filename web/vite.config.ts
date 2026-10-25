import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import { tanstackRouter } from "@tanstack/router-plugin/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { precacheShell } from "./scripts/sw-precache";

export default defineConfig({
  plugins: [
    tanstackRouter({
      target: "react",
      routesDirectory: "src/router/routes",
      autoCodeSplitting: true,
    }),
    react(),
    tailwindcss(),
    precacheShell(),
  ],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  server: {
    port: 3000,
    proxy: {
      "/api": {
        target: process.env.VITE_API_BASE_URL ?? "http://localhost:8095",
        changeOrigin: true,
      },
    },
  },
});
