import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    host: "0.0.0.0",
    port: 54001,
    proxy: {
      "/api": { target: "http://127.0.0.1:54000", ws: true },
      "/ws": { target: "http://127.0.0.1:54000", ws: true },
    },
  },
});
