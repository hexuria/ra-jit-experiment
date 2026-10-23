import { sveltekit } from "@sveltejs/kit/vite";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { defineConfig } from "vite";

// The benchmark history /bench reads at build time; BENCH_HISTORY points it at another directory of .jsonl files.
const history = process.env.BENCH_HISTORY
  ? path.resolve(process.env.BENCH_HISTORY)
  : fileURLToPath(new URL("../docs/benchmarks/history", import.meta.url));

export default defineConfig({
  plugins: [sveltekit()],
  resolve: { alias: { $history: history } },
});
