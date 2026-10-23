<script lang="ts">
	type Props = { label: string; options: { value: string; label: string }[]; value: string; onchange: (value: string) => void };
	let { label, options, value, onchange }: Props = $props();
</script>

{#if options.length > 1}
	<div class="choice" role="group" aria-label={label}>
		{#each options as o (o.value)}
			<button type="button" aria-pressed={o.value === value} onclick={() => onchange(o.value)}>{o.label}</button>
		{/each}
	</div>
{/if}

<style>
	.choice { display: flex; flex-wrap: wrap; gap: 6px; margin: 0 0 12px; }
	button {
		padding: 5px 12px;
		border: 0;
		border-radius: 999px;
		background: transparent;
		box-shadow: inset 0 0 0 1px var(--line);
		color: var(--ink-2);
		font: inherit;
		font-size: 14px;
		line-height: 1.4;
		text-align: left;
		cursor: pointer;
		transition: background 0.15s, color 0.15s, box-shadow 0.15s;
	}
	button:hover { box-shadow: inset 0 0 0 1px var(--line-2); color: var(--ink-1); }
	button[aria-pressed='true'] { background: var(--ink-1); box-shadow: none; color: var(--bg); font-weight: 500; }
</style>
