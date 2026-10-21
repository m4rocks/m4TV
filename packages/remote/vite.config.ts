import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// es2020 and iOS 14 (ADR 0004): no legacy build.
export default defineConfig({
  plugins: [react()],
  build: { target: ["es2020", "safari14"] },
});
