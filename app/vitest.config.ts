// Tests run without the app's CSS plugins (vite.config.ts), only the Svelte-free modules in src/lib.
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    // Only used by `--coverage` (scripts/code-health.py): count every lib module, tested or not.
    coverage: { provider: "v8", include: ["src/lib/**/*.ts"], exclude: ["src/lib/**/*.test.ts", "src/lib/bindings/**"] },
  },
});
