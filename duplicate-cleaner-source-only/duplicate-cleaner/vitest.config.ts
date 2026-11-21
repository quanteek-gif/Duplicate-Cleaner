import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Vitest shares the Vite resolution so tests import the same modules the app
// does. jsdom is only needed for the few tests that touch localStorage.
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    include: ["src/**/*.{test,spec}.{ts,tsx}"],
    reporters: "default",
  },
});
