import { parseHistory } from '$lib/bench/data';
import type { PageServerLoad } from './$types';

export const prerender = true;

// Read when the site is built: a new history line reaches the page with the next deploy. `$history` is
// docs/benchmarks/history, or the directory BENCH_HISTORY names (vite.config.js).
const files = import.meta.glob<string>('$history/*.jsonl', { query: '?raw', import: 'default', eager: true });

export const load: PageServerLoad = () => parseHistory(Object.values(files));
