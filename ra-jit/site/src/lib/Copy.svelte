<script lang="ts">
	import { highlight } from './highlight';
	let { text }: { text: string } = $props();
	let done = $state(false);
	async function copy() {
		await navigator.clipboard.writeText(text);
		done = true;
		setTimeout(() => (done = false), 1500);
	}
</script>

<div class="cmd code-roles">
	<pre class="hljs"><code>{@html highlight(text, 'bash')}</code></pre>
	<button onclick={copy} aria-label="Copy command">{done ? 'Copied' : 'Copy'}</button>
</div>

<style>
	.cmd { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin: 0 0 1.3em; padding: 4px 8px 4px 16px; border-radius: 12px; background: var(--code-bg); box-shadow: inset 0 0 0 1px var(--line); }
	pre { margin: 0; padding: 10px 0; overflow-x: auto; font-size: 14px; }
	code { font-family: var(--font-mono); background: none; padding: 0; }
	button { flex: none; padding: 6px 12px; border: 0; border-radius: 8px; background: var(--s-tint); color: var(--ink-1); font: inherit; font-size: 13px; cursor: pointer; }
	button:hover { background: var(--bg-4); }
</style>
