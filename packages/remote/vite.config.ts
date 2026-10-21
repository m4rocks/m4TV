import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// es2020 and iOS 14 (ADR 0004): no legacy build. The fonts come from design/public (ADR 0005).
export default defineConfig({
  plugins: [react()],
  publicDir: "../../design/public",
  build: { target: ["es2020", "safari14"] },
});
