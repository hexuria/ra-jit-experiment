<script lang="ts" module>
	export type Series = {
		label: string;
		/** ra-jit's numbers are drawn bright, the ones it is compared with dim. */
		tone: 'ours' | 'other';
		dashed?: boolean;
		values: (number | null | undefined)[];
		/** The fastest and slowest of the run's samples, drawn as a band. */
		lo?: (number | null | undefined)[];
		hi?: (number | null | undefined)[];
	};
	export type Point = { date: string; commit?: string };
</script>

<script lang="ts">
	import { commitUrl, day, short } from './format';
	import { linearDomain, linearTicks, logDomain, logTicks } from './scale';

	type Props = {
		title: string;
		series: Series[];
		runs: Point[];
		active: number;
		onactive: (index: number) => void;
		scale?: 'log' | 'linear';
		format: (v: number) => string;
		tick: (v: number) => string;
		/** A domain shared with other charts, so that they read alike. */
		domain?: [number, number] | null;
		empty?: string;
	};
	let { title, series, runs, active, onactive, scale = 'log', format, tick, domain, empty = 'No data yet.' }: Props = $props();

	const W = 340;
	const H = 168;
	const M = { l: 52, r: 12, t: 10, b: 24 };
	const PW = W - M.l - M.r;
	const PH = H - M.t - M.b;
	const DOTS_UP_TO = 24;

	const defined = (v: number | null | undefined): v is number =>
		typeof v === 'number' && Number.isFinite(v) && (scale === 'linear' || v > 0);

	let n = $derived(runs.length);
	// ra-jit's series last, so that it is drawn on top.
	let drawn = $derived([...series].sort((a, b) => Number(a.tone === 'ours') - Number(b.tone === 'ours')));
	let all = $derived(series.flatMap((s) => [...s.values, ...(s.lo ?? []), ...(s.hi ?? [])]));
	let hasData = $derived(series.some((s) => s.values.some(defined)));
	let dom = $derived(domain ?? (scale === 'log' ? logDomain(all) : linearDomain(all)) ?? ([1, 10] as [number, number]));
	let ticks = $derived(scale === 'log' ? logTicks(dom) : linearTicks(dom));

	const x = (i: number) => (n <= 1 ? M.l + PW / 2 : M.l + (i / (n - 1)) * PW);
	function y(v: number) {
		const [lo, hi] = dom;
		const t = scale === 'log' ? (Math.log(v) - Math.log(lo)) / (Math.log(hi) - Math.log(lo)) : (v - lo) / (hi - lo);
		return M.t + PH - Math.min(1, Math.max(0, t)) * PH;
	}

	/** Runs of consecutive indices where `ok(i)`: a missing run breaks the line. */
	function stretches(ok: (i: number) => boolean) {
		const out: number[][] = [];
		let cur: number[] = [];
		for (let i = 0; i < n; i++) {
			if (ok(i)) cur.push(i);
			else if (cur.length) (out.push(cur), (cur = []));
		}
		if (cur.length) out.push(cur);
		return out;
	}

	function line(s: Series) {
		return stretches((i) => defined(s.values[i]))
			.filter((st) => st.length > 1)
			.map((st) => st.map((i, k) => `${k ? 'L' : 'M'}${x(i).toFixed(1)},${y(s.values[i]!).toFixed(1)}`).join(''));
	}

	function bands(s: Series) {
		if (!s.lo || !s.hi) return { areas: [] as string[], whiskers: [] as number[] };
		const lo = s.lo;
		const hi = s.hi;
		const st = stretches((i) => defined(lo[i]) && defined(hi[i]));
		const areas = st
			.filter((r) => r.length > 1)
			.map((r) => {
				const top = r.map((i) => `${x(i).toFixed(1)},${y(hi[i]!).toFixed(1)}`);
				const bottom = [...r].reverse().map((i) => `${x(i).toFixed(1)},${y(lo[i]!).toFixed(1)}`);
				return `M${top.join('L')}L${bottom.join('L')}Z`;
			});
		return { areas, whiskers: st.filter((r) => r.length === 1).map((r) => r[0]) };
	}

	let el: SVGSVGElement | undefined = $state();

	function pick(event: PointerEvent) {
		if (!el || n === 0) return;
		const box = el.getBoundingClientRect();
		const px = ((event.clientX - box.left) / box.width) * W;
		const i = n <= 1 ? 0 : Math.round(((px - M.l) / PW) * (n - 1));
		onactive(Math.min(n - 1, Math.max(0, i)));
	}

	function key(event: KeyboardEvent) {
		const to = { ArrowLeft: active - 1, ArrowRight: active + 1, Home: 0, End: n - 1 }[event.key];
		if (to === undefined) return;
		event.preventDefault();
		onactive(Math.min(n - 1, Math.max(0, to)));
	}

	let run = $derived(runs[active]);
	let readout = $derived(series.map((s) => ({ s, v: s.values[active] })));
	let valuetext = $derived(
		[run ? `${short(run.commit)}, ${day(run.date)}` : '', ...readout.map(({ s, v }) => `${s.label} ${defined(v) ? format(v) : 'not measured'}`)].join(', '),
	);
</script>

<figure class="chart">
	<figcaption>
		<span class="title">{title}</span>
		{#if hasData}
			<span class="values">
				{#each readout as { s, v } (s.label)}
					<span class="value">
						<svg class="key" viewBox="0 0 14 6" aria-hidden="true"><line x1="0" y1="3" x2="14" y2="3" class={s.tone} class:dashed={s.dashed} /></svg>
						{s.label} <b class={s.tone}>{defined(v) ? format(v) : '–'}</b>
					</span>
				{/each}
			</span>
		{/if}
	</figcaption>
	{#if hasData}
		<svg
			bind:this={el}
			viewBox="0 0 {W} {H}"
			role="slider"
			tabindex="0"
			aria-label="{title}: pick a run"
			aria-valuemin={1}
			aria-valuemax={n}
			aria-valuenow={active + 1}
			aria-valuetext={valuetext}
			onpointermove={pick}
			onpointerdown={pick}
			onkeydown={key}
		>
			{#each ticks as t (t)}
				<line class="grid" x1={M.l} x2={W - M.r} y1={y(t)} y2={y(t)} />
				<text class="tick" x={M.l - 7} y={y(t)} dy="0.32em" text-anchor="end">{tick(t)}</text>
			{/each}
			{#if n > 1}
				<text class="tick" x={M.l} y={H - 6} text-anchor="start">{day(runs[0].date)}</text>
				<text class="tick" x={W - M.r} y={H - 6} text-anchor="end">{day(runs[n - 1].date)}</text>
			{:else if n === 1}
				<text class="tick" x={x(0)} y={H - 6} text-anchor="middle">{day(runs[0].date)}</text>
			{/if}
			{#if n > 1}<line class="guide" x1={x(active)} x2={x(active)} y1={M.t} y2={M.t + PH} />{/if}

			{#each drawn as s (s.label)}
				{@const b = bands(s)}
				{#each b.areas as d}<path class="band {s.tone}" {d} />{/each}
				{#each b.whiskers as i}
					<line class="whisker {s.tone}" x1={x(i)} x2={x(i)} y1={y(s.hi![i]!)} y2={y(s.lo![i]!)} />
					<line class="whisker {s.tone}" x1={x(i) - 3} x2={x(i) + 3} y1={y(s.hi![i]!)} y2={y(s.hi![i]!)} />
					<line class="whisker {s.tone}" x1={x(i) - 3} x2={x(i) + 3} y1={y(s.lo![i]!)} y2={y(s.lo![i]!)} />
				{/each}
			{/each}
			{#each drawn as s (s.label)}
				{#each line(s) as d}<path class="line {s.tone}" class:dashed={s.dashed} {d} />{/each}
			{/each}
			{#each drawn as s (s.label)}
				{#each s.values as v, i}
					{#if defined(v) && (n <= DOTS_UP_TO || i === active)}
						{@const r = runs[i]}
						<a href={r.commit ? commitUrl(r.commit) : undefined} tabindex="-1" aria-hidden="true">
							<circle class="dot {s.tone}" class:on={i === active} cx={x(i)} cy={y(v)} r={i === active ? 3.6 : 2.2}>
								<title>{s.label} {format(v)}, {short(r.commit)}, {day(r.date)}</title>
							</circle>
						</a>
					{/if}
				{/each}
			{/each}
		</svg>
	{:else}
		<p class="empty">{empty}</p>
	{/if}
</figure>

<style>
	.chart {
		margin: 0;
		padding: 12px 12px 6px;
		border-radius: 12px;
		background: var(--code-bg);
		box-shadow: inset 0 0 0 1px var(--line);
		min-width: 0;
	}
	figcaption { display: flex; flex-direction: column; gap: 2px; margin: 0 2px 6px; }
	.title { color: var(--ink-1); font-size: 14px; font-weight: 600; line-height: 1.35; }
	.values { display: flex; flex-wrap: wrap; gap: 2px 14px; font-size: 13px; line-height: 1.5; color: var(--ink-3); font-variant-numeric: tabular-nums; }
	.value { white-space: nowrap; }
	.value b { font-weight: 500; }
	.value b.ours { color: var(--ink-1); }
	.value b.other { color: var(--ink-2); }
	.key { display: inline-block; width: 14px; height: 6px; margin-right: 3px; vertical-align: middle; overflow: visible; }
	svg[role='slider'] { display: block; width: 100%; height: auto; touch-action: pan-y; cursor: crosshair; border-radius: 6px; }
	svg[role='slider']:focus-visible { outline-offset: 4px; }
	.grid { stroke: var(--line); stroke-width: 1; }
	.tick { fill: var(--ink-3); font-size: 11px; font-variant-numeric: tabular-nums; }
	.guide { stroke: var(--line-2); stroke-width: 1; }
	.line, .key line { fill: none; stroke-width: 1.75; stroke-linejoin: round; stroke-linecap: round; }
	.dashed { stroke-dasharray: 4 3; }
	.ours { stroke: var(--ink-1); }
	.other { stroke: var(--ink-3); }
	.band { stroke: none; }
	.band.ours { fill: color-mix(in srgb, var(--ink-1) 13%, transparent); }
	.band.other { fill: color-mix(in srgb, var(--ink-3) 18%, transparent); }
	.whisker { stroke-width: 1; }
	.whisker.ours { stroke: color-mix(in srgb, var(--ink-1) 55%, transparent); }
	.whisker.other { stroke: color-mix(in srgb, var(--ink-3) 70%, transparent); }
	.dot { stroke: var(--code-bg); stroke-width: 1.25; }
	.dot.ours { fill: var(--ink-1); }
	.dot.other { fill: var(--ink-3); }
	.dot.on { stroke-width: 2; }
	a { cursor: pointer; }
	.empty { display: grid; place-items: center; aspect-ratio: 340 / 168; margin: 0; color: var(--ink-3); font-size: 14px; }
</style>
