<script lang="ts">
	/* A code block after ix lib/docs/Code.svelte: one box, an optional title bar, coloured with the docs' roles. */
	import { highlight } from './highlight';

	let { lang, title, text }: { lang: 'rust' | 'toml' | 'bash'; title?: string; text: string } = $props();
</script>

<figure class="codeblock code-roles">
	{#if title !== undefined}<figcaption><span class="title mono">{title}</span></figcaption>{/if}
	<pre class="hljs" data-lang={lang}><code>{@html highlight(text, lang)}</code></pre>
</figure>

<style>
	.codeblock { position: relative; min-width: 0; margin: 0 0 1.3em; border-radius: 12px; background: var(--code-bg); box-shadow: inset 0 0 0 1px var(--line); overflow: hidden; }
	pre { margin: 0; padding: 14px 16px; overflow-x: auto; font-size: 13.5px; line-height: 1.6; tab-size: 4; }
	code { font-family: var(--font-mono); font-size: inherit; background: none; padding: 0; }
	figcaption { display: flex; align-items: center; min-height: 38px; padding: 0 16px; border-bottom: 1px solid var(--line); }
	.title { font-size: 12.5px; color: var(--ink-3); }
	@media (max-width: 480px) { pre { font-size: 12px; padding: 12px; } }
</style>
