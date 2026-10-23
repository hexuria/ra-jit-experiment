<script lang="ts">
	import Chart, { type Series } from '$lib/bench/Chart.svelte';
	import Choice from '$lib/bench/Choice.svelte';
	import { backendOrder, type Entry, type Point, type Run } from '$lib/bench/data';
	import {
		BACKEND_NAMES, EDITS, EDIT_ORDER, MODES, PROGRAMS, REPO, SETUPS, SETUP_ORDER,
		commitUrl, megabytes, megabytesTick, millis, millisTick, projectName, seconds, secondsTick, short, speedup, when,
	} from '$lib/bench/format';
	import { logDomain } from '$lib/bench/scale';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const ISSUES = `${REPO}/issues?q=label%3Abench-regression`;
	const LAST = Number.MAX_SAFE_INTEGER;

	const ranked = (order: string[]) => (a: string, b: string) =>
		(order.includes(a) ? order.indexOf(a) : order.length) - (order.includes(b) ? order.indexOf(b) : order.length) || a.localeCompare(b);
	const unique = (items: string[]) => [...new Set(items)];
	const projectOf = (key: string) => key.split('/')[0];
	const backendOf = (key: string) => key.split('/')[1] ?? '';
	const backendOptions = (keys: string[]) => unique(keys.map(backendOf)).sort(backendOrder).map((b) => ({ value: b, label: BACKEND_NAMES[b] ?? b }));
	const pickOr = (wanted: string, options: { value: string }[]) => (options.some((o) => o.value === wanted) ? wanted : (options[0]?.value ?? ''));
	const machineLabel = (name: string) => name || 'Unnamed machine';
	const cpuLine = (run: Run) =>
		[run.cpu?.replace(/( \d+-Core)? Processor$/, ''), run.cores ? `${run.cores} cores` : undefined].filter(Boolean).join(', ');

	// Latest run, of the platform chosen.
	let platforms = $derived(data.latest.map((l) => ({ value: l.run.platform, label: l.run.platform })));
	let platformWanted = $state('Linux x86-64');
	let latest = $derived(data.latest.find((l) => l.run.platform === platformWanted) ?? data.latest.at(-1));
	let latestKeys = $derived(latest ? Object.keys(latest.results) : []);
	let summaryBackends = $derived(backendOptions(latestKeys));
	let summaryBackendWanted = $state('cranelift');
	let summaryBackend = $derived(pickOr(summaryBackendWanted, summaryBackends));
	let summarySetups = $derived(
		unique(
			latestKeys
				.filter((k) => backendOf(k) === summaryBackend)
				.flatMap((k) => Object.values(latest!.results[k].phases).flatMap((p) => Object.keys(p.setups ?? {}))),
		).sort(ranked(SETUP_ORDER)),
	);

	type Row = {
		label: string;
		cargo?: number;
		ours?: number;
		cargoMem?: number;
		oursMem?: number;
		fallback: boolean;
		setups: Record<string, number>;
		fastest?: number;
	};
	function table(entry: Entry) {
		const phases = Object.keys(entry.phases).sort(ranked(EDIT_ORDER));
		const setups = unique(phases.flatMap((p) => Object.keys(entry.phases[p].setups ?? {}))).sort(ranked(SETUP_ORDER));
		const rows: Row[] = phases.map((p) => {
			const ph = entry.phases[p];
			const builds = [ph.cargo_s, ph.wall_s, ...Object.entries(ph.setups ?? {}).filter(([s]) => s !== 'check').map(([, v]) => v)];
			const known = builds.filter((v): v is number => v !== undefined);
			return {
				label: EDITS[p] ?? p,
				cargo: ph.cargo_s,
				ours: ph.wall_s,
				cargoMem: ph.cargo_mem_mb,
				oursMem: ph.mem_mb,
				fallback: p !== 'cold' && !!ph.built && ph.built !== 'daemon',
				setups: ph.setups ?? {},
				fastest: known.length ? Math.min(...known) : undefined,
			};
		});
		const memory = rows.some((r) => r.oursMem !== undefined || r.cargoMem !== undefined);
		return { setups, rows, memory };
	}
	/** The fastest way cargo built it, of plain cargo and the setups (not check, which builds no program). */
	const fastestCargo = (row: Row) => {
		const all = [row.cargo, ...Object.entries(row.setups).filter(([s]) => s !== 'check').map(([, v]) => v)].filter((v): v is number => v !== undefined);
		return all.length ? Math.min(...all) : undefined;
	};

	// History of build runs.
	let buildMachines = $derived(data.build.map((m) => ({ value: m.machine, label: `${m.platform}: ${machineLabel(m.machine)}` })));
	let buildMachineWanted = $state('');
	let buildMachine = $derived(
		data.build.find((m) => m.machine === buildMachineWanted) ??
			data.build.find((m) => m.machine === (latest?.run.machine ?? '')) ??
			data.build.at(-1),
	);
	let projects = $derived(unique(buildMachine?.keys.map(projectOf) ?? []).map((p) => ({ value: p, label: p })));
	let projectWanted = $state('ripgrep');
	let project = $derived(pickOr(projectWanted, projects));
	let backends = $derived(backendOptions((buildMachine?.keys ?? []).filter((k) => projectOf(k) === project)));
	let backendWanted = $state('cranelift');
	let backend = $derived(pickOr(backendWanted, backends));

	let runs = $derived(buildMachine?.runs ?? []);
	let history = $derived(buildMachine?.results[`${project}/${backend}`]);
	let activeWanted = $state(LAST);
	let active = $derived(Math.max(0, Math.min(activeWanted, runs.length - 1)));
	let activeRun = $derived(runs[active]);

	let editCharts = $derived(
		Object.keys(history?.phases ?? {})
			.filter((p) => p !== 'cold')
			.sort(ranked(EDIT_ORDER))
			.map((p) => {
				const ph = history!.phases[p];
				const series: Series[] = [
					{ label: 'ra-jit', tone: 'ours', values: ph.wall, lo: ph.min, hi: ph.max },
					{ label: 'cargo', tone: 'other', values: ph.cargo, lo: ph.cargoMin, hi: ph.cargoMax },
				];
				return { phase: p, title: EDITS[p] ?? p, series };
			}),
	);
	let editDomain = $derived(logDomain(editCharts.flatMap((c) => c.series.flatMap((s) => [...s.values, ...(s.lo ?? []), ...(s.hi ?? [])]))));
	let memoryCharts = $derived(
		Object.keys(history?.phases ?? {})
			.sort(ranked(EDIT_ORDER))
			.filter((p) => history!.phases[p].mem.length || history!.phases[p].cargoMem.length)
			.map((p) => {
				const ph = history!.phases[p];
				const series: Series[] = [
					{ label: 'ra-jit', tone: 'ours', values: ph.mem },
					{ label: 'cargo', tone: 'other', values: ph.cargoMem },
				];
				return { phase: p, title: EDITS[p] ?? p, series };
			}),
	);
	let memoryDomain = $derived(logDomain(memoryCharts.flatMap((c) => c.series.flatMap((s) => s.values))));
	let cold = $derived(history?.phases.cold);

	// History of live edits.
	let liveMachines = $derived(data.live.map((m) => ({ value: m.machine, label: machineLabel(m.machine) })));
	let liveMachineWanted = $state('');
	let liveMachine = $derived(data.live.find((m) => m.machine === liveMachineWanted) ?? data.live.at(-1));
	let liveRuns = $derived(liveMachine?.runs ?? []);
	let liveActiveWanted = $state(LAST);
	let liveActive = $derived(Math.max(0, Math.min(liveActiveWanted, liveRuns.length - 1)));

	let capped = $derived([...data.build, ...data.live].some((m) => m.runs.length >= data.maxRuns));
</script>

<svelte:head>
	<title>Benchmark history · ra-jit</title>
	<meta name="description" content="ra-jit's build and live-edit benchmarks over every run, next to plain cargo on the same machine." />
</svelte:head>

{#snippet runLine(run: Point, index: number, count: number)}
	<p class="run">
		<span class="dim">Run {index + 1} of {count}:</span>
		{#if run.commit}<a href={commitUrl(run.commit)}><code>{short(run.commit)}</code></a>{:else}unknown commit{/if}
		<span class="dim">on {when(run.date)}</span>{#if run.runUrl}<span class="dim">{' · '}</span><a href={run.runUrl}>CI run</a>{/if}
	</p>
{/snippet}

<main class="doc">
	<p class="back"><a href="/">ra-jit</a></p>
	<h1>Benchmark history</h1>
	<p class="lede">
		How long ra-jit takes to build after an edit, next to plain cargo on the same machine, on real projects at pinned commits
		(<a href="{REPO}/blob/main/docs/benchmarks.md">how it is measured</a>), and how soon a live edit runs. Each platform,
		macOS arm64, Linux aarch64 and Linux x86-64, is measured on a machine of its own, so compare its numbers with each other,
		not with another platform's. It updates after every push to main and every night.
	</p>

	<h2>Latest run</h2>
	{#if latest}
		{@const run = latest.run}
		{#if platforms.length > 1}
			<Choice label="Platform" options={platforms} value={run.platform} onchange={(v) => ((platformWanted = v), (buildMachineWanted = ''))} />
		{/if}
		<p>
			{run.platform}, {machineLabel(run.machine ?? '')}{#if cpuLine(run)}&nbsp;({cpuLine(run)}){/if}, ra-jit
			{#if run.commit}<a href={commitUrl(run.commit)}><code>{short(run.commit)}</code></a>{:else}(commit unknown){/if}
			on {when(run.date)}{#if run.runs}, median of {run.runs} edits{/if}.
			{#if run.runUrl}<a href={run.runUrl}>The CI run</a>.{/if}
		</p>
		<Choice label="Backend" options={summaryBackends} value={summaryBackend} onchange={(v) => (summaryBackendWanted = v)} />
		{#each latestKeys.filter((k) => backendOf(k) === summaryBackend) as key (key)}
			{@const entry = latest.results[key]}
			{@const t = table(entry)}
			<h3>{projectName(projectOf(key), entry.tag)}</h3>
			<div class="ref" class:wide={t.setups.length > 0}>
				<table>
					<thead>
						<tr>
							<th>Edit</th>
							<th class="num">cargo</th>
							<th class="num">ra-jit</th>
							<th class="num">Faster</th>
							{#if t.memory}<th class="num"><abbr title="The most every process doing the work held at once">cargo memory</abbr></th><th class="num"><abbr title="The daemon, its workers and the command, at their peak">ra-jit memory</abbr></th>{/if}
							{#each t.setups as s (s)}<th class="num"><abbr title={SETUPS[s]?.[1] ?? s}>{SETUPS[s]?.[0] ?? s}</abbr></th>{/each}
							{#if t.setups.length}<th class="num">vs fastest cargo</th>{/if}
						</tr>
					</thead>
					<tbody>
						{#each t.rows as row (row.label)}
							<tr>
								<td>{row.label}</td>
								<td class="num" class:best={row.cargo === row.fastest}>{seconds(row.cargo)}</td>
								<td class="num" class:best={row.ours === row.fastest}>
									{seconds(row.ours)}{#if row.fallback}<abbr class="mark" title="The daemon handed this edit to cargo">&nbsp;(c)</abbr>{/if}
								</td>
								<td class="num ratio">{speedup(row.cargo, row.ours)}</td>
								{#if t.memory}<td class="num">{megabytes(row.cargoMem)}</td><td class="num">{megabytes(row.oursMem)}</td>{/if}
								{#each t.setups as s (s)}
									<td class="num" class:best={s !== 'check' && row.setups[s] === row.fastest}>{seconds(row.setups[s])}</td>
								{/each}
								{#if t.setups.length}<td class="num ratio">{speedup(fastestCargo(row), row.ours)}</td>{/if}
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/each}
		<p class="dim note">
			Bold is the fastest of each row.
			{#if summarySetups.length}
				The columns after Faster are cargo on the same edits, set up other ways:
				{summarySetups.map((s) => `${SETUPS[s]?.[0] ?? s} is ${SETUPS[s]?.[1] ?? s}`).join('; ')}.
			{/if}
			{#if latestKeys.some((k) => Object.entries(latest.results[k].phases).some(([p, ph]) => p !== 'cold' && ph.built && ph.built !== 'daemon'))}
				(c): the daemon handed the edit to cargo, so it took about cargo's time.
			{/if}
		</p>
	{:else}
		<p class="dim">No benchmark runs recorded yet.</p>
	{/if}

	<h2>Over time</h2>
	{#if buildMachine && runs.length}
		<Choice label="Machine" options={buildMachines} value={buildMachine.machine} onchange={(v) => ((buildMachineWanted = v), (activeWanted = LAST))} />
		<Choice label="Project" options={projects} value={project} onchange={(v) => (projectWanted = v)} />
		<Choice label="Backend" options={backends} value={backend} onchange={(v) => (backendWanted = v)} />
		{@render runLine(activeRun, active, runs.length)}

		<h3>{projectName(project, history?.tag)}: edit, then build</h3>
		<p class="dim note">
			Each point is a run's median, the band its fastest to slowest edit. Time is on a log scale, so equal steps are equal
			ratios. Point at a chart, or tap it, to read a run; click a point to open its commit.
		</p>
		<div class="grid">
			{#each editCharts as c (c.phase)}
				<Chart title={c.title} series={c.series} {runs} {active} onactive={(i) => (activeWanted = i)} format={seconds} tick={secondsTick} domain={editDomain} />
			{/each}
		</div>

		{#if memoryCharts.length}
			<h3>{projectName(project, history?.tag)}: memory while it builds</h3>
			<p class="dim note">
				The most every process doing the work held at once: for ra-jit the daemon, its workers and the command; for plain
				cargo, cargo itself, rustc and the linker (<a href="{REPO}/blob/main/docs/benchmarks.md#memory-and-disk">how</a>). Log scale.
			</p>
			<div class="grid">
				{#each memoryCharts as c (c.phase)}
					<Chart title={c.title} series={c.series} {runs} {active} onactive={(i) => (activeWanted = i)} format={megabytes} tick={megabytesTick} domain={memoryDomain} />
				{/each}
			</div>
		{/if}

		<h3>Cold build, memory and store</h3>
		<div class="grid">
			<Chart
				title="Cold build: cargo, and ra-jit until ready"
				series={[
					{ label: 'ra-jit', tone: 'ours', values: cold?.wall ?? [] },
					...(cold?.warmStore.some((v) => v !== null)
						? [{ label: 'ra-jit, warm store', tone: 'ours' as const, dashed: true, values: cold.warmStore }]
						: []),
					{ label: 'cargo', tone: 'other', values: cold?.cargo ?? [] },
				]}
				{runs} {active} onactive={(i) => (activeWanted = i)} scale="linear" format={seconds} tick={secondsTick}
			/>
			<Chart
				title="Daemon between edits"
				series={[
					{ label: 'after the edits', tone: 'ours', values: history?.residentEnd ?? [] },
					{ label: 'once ready', tone: 'other', dashed: true, values: history?.residentReady ?? [] },
				]}
				{runs} {active} onactive={(i) => (activeWanted = i)} scale="linear" format={megabytes} tick={megabytesTick}
				empty="Not measured in these runs yet."
			/>
			<Chart
				title="On disk"
				series={[
					{ label: 'ra-jit: target, store, home', tone: 'ours', values: history?.disk ?? [] },
					{ label: 'cargo: target', tone: 'other', values: history?.cargoDisk ?? [] },
				]}
				{runs} {active} onactive={(i) => (activeWanted = i)} scale="linear" format={megabytes} tick={megabytesTick}
				empty="Not measured in these runs yet."
			/>
			<Chart
				title="Daemon peak memory"
				series={[{ label: 'ra-jit', tone: 'ours', values: history?.memory ?? [] }]}
				{runs} {active} onactive={(i) => (activeWanted = i)} scale="linear" format={megabytes} tick={megabytesTick}
			/>
			<Chart
				title="Store size"
				series={[{ label: 'ra-jit', tone: 'ours', values: history?.store ?? [] }]}
				{runs} {active} onactive={(i) => (activeWanted = i)} scale="linear" format={megabytes} tick={megabytesTick}
				empty="Not measured in these runs yet."
			/>
		</div>
	{:else}
		<p class="dim">No benchmark runs recorded yet.</p>
	{/if}

	<h2>Live edits</h2>
	{#if liveMachine && liveRuns.length}
		<p>
			Edit to running: from the edit leaving the editor to the new code running in the program, on
			{machineLabel(liveMachine.machine)}. Solid is the median edit, dashed the 90th percentile.
		</p>
		<Choice label="Machine" options={liveMachines} value={liveMachine.machine} onchange={(v) => ((liveMachineWanted = v), (liveActiveWanted = LAST))} />
		{@render runLine(liveRuns[liveActive], liveActive, liveRuns.length)}
		{#each liveMachine.programs as program (program)}
			<h3>{PROGRAMS[program] ?? program}</h3>
			<div class="grid">
				{#each liveMachine.modes[program] as mode (mode)}
					{@const stats = liveMachine.results[program][mode]}
					<Chart
						title={MODES[mode] ?? mode}
						series={[
							{ label: 'median', tone: 'ours', values: stats.p50 },
							{ label: '90th percentile', tone: 'other', dashed: true, values: stats.p90 },
						]}
						runs={liveRuns} active={liveActive} onactive={(i) => (liveActiveWanted = i)} format={millis} tick={millisTick}
					/>
				{/each}
			</div>
		{/each}
	{:else}
		<p class="dim">No live-edit runs recorded yet.</p>
	{/if}

	{#if capped}<p class="dim note">The charts show the last {data.maxRuns} runs of each machine.</p>{/if}

	<h2>Regressions</h2>
	<p>
		Each run is compared with the runs before it on the same machine. A number that goes above its noise band opens an
		issue labelled <a href={ISSUES}>bench-regression</a>.
	</p>
	<p class="dim">
		The data: <a href="{REPO}/tree/main/docs/benchmarks/history">docs/benchmarks/history</a>, one line per run.
	</p>
</main>

<style>
	main { max-width: 760px; margin: 0 auto; padding: 64px 20px 96px; }
	.back { margin: 0 0 1.5em; font-size: 14px; }
	h1 { margin: 0 0 0.2em; font-size: 44px; font-weight: 700; letter-spacing: -0.03em; color: var(--ink-1); }
	.lede { font-size: 18px; line-height: 1.55; color: var(--ink-1); }
	.lede a { color: inherit; }
	.note { font-size: 14px; line-height: 1.55; }
	.run { margin: 4px 0 0; font-size: 14px; font-variant-numeric: tabular-nums; }
	.grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; margin: 0 0 1.3em; }
	.ref th, .ref td { white-space: nowrap; }
	.ref .num { text-align: right; }
	/* With the setups the table scrolls sideways; the edit stays in view. */
	.wide th:first-child, .wide td:first-child { position: sticky; left: 0; z-index: 1; background: var(--bg); box-shadow: inset -1px 0 0 var(--line); }
	.wide th:first-child { background: color-mix(in srgb, var(--ink-1) 6%, var(--bg)); }
	.ref td.best { color: var(--ink-1); font-weight: 600; }
	.ref td.ratio { color: var(--ink-3); }
	.ref abbr { text-decoration: none; cursor: help; }
	.mark { color: var(--ink-3); font-weight: 400; }
	@media (max-width: 620px) {
		main { padding-top: 40px; }
		h1 { font-size: 36px; }
		.grid { grid-template-columns: minmax(0, 1fr); }
		.ref td:first-child { white-space: normal; min-width: 8.5em; }
		.wide td:first-child { max-width: 9.5em; }
		.ref th, .ref td { padding-left: 10px; padding-right: 10px; }
	}
</style>
