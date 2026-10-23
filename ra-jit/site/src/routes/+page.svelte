<script lang="ts">
	import Code from '$lib/Code.svelte';
	import Copy from '$lib/Copy.svelte';

	const poll = `let mut live = ra_jit_live::Live::start().ok();
loop {
    if let Some(outcome) = live.as_mut().and_then(|it| it.poll()) {
        eprintln!("ra-jit: {outcome}");
    }
    // ...your frame
}`;
</script>

<svelte:head>
	<title>ra-jit: edit Rust, see it change in milliseconds</title>
	<meta name="description" content="Edit Rust. See it change in milliseconds, in the program that is already running." />
</svelte:head>

<main class="doc">
	<h1>ra-jit</h1>
	<p class="lede">Edit Rust. See it change in milliseconds, in the program that is already running.</p>

	<Copy text="nix run github:indexable-inc/ra-jit" />

	<img src="/demo.avif" width="2560" height="1440" alt="Editing the running Game of Life: rule, colours and speed change live" />

	<h2>What you get</h2>
	<ul>
		<li><strong>Live edits.</strong> Save a file and the running program has the new code about a millisecond later. Its state stays.</li>
		<li><strong>Fast builds.</strong> After the first build, an edit rebuilds only the functions you changed. No <code>rustc</code>, no waiting.</li>
		<li><strong>Same cargo.</strong> <code>cargo ra-jit build|run|test|check</code> in any project.</li>
	</ul>

	<h2>How fast</h2>
	<p class="dim">Edit one function, then build. 200-crate workspace, median of three.</p>
	<div class="ref">
		<table>
			<thead><tr><th>Edit</th><th>cargo</th><th>ra-jit</th></tr></thead>
			<tbody>
				<tr><td>Function in the main crate</td><td>1.76 s</td><td>0.42 s</td></tr>
				<tr><td>Function in a crate everything depends on</td><td>6.33 s</td><td>0.60 s</td></tr>
			</tbody>
		</table>
	</div>
	<p class="dim"><a href="/bench">Benchmark history</a>: every run on real projects, next to plain cargo.</p>

	<h2>Try it</h2>
	<Code lang="bash" text="nix run github:indexable-inc/ra-jit -- build" />
	<h3>Use it as your normal cargo build</h3>
	<Code lang="bash" text={'nix profile install github:indexable-inc/ra-jit\nra-jit enable'} />
	<p class="dim">Plain <code>cargo build</code>, <code>run</code>, <code>test</code> and <code>check</code> then go through ra-jit. <code>ra-jit disable</code> undoes it.</p>
	<h3>Live edits</h3>
	<Code lang="toml" text={'ra-jit-live = { git = "https://github.com/indexable-inc/ra-jit" }'} />
	<Code lang="rust" text={poll} />

	<h2>What to know</h2>
	<ul>
		<li>Live edits: macOS Apple silicon only. Elsewhere you still get fast builds.</li>
		<li>No borrow checker: keep running <code>cargo check</code>.</li>
		<li>Rust 1.99.0 only.</li>
		<li>Changing a struct the program is using asks for a restart.</li>
	</ul>
	<p>
		More: <a href="https://github.com/indexable-inc/ra-jit/blob/main/docs/architecture.md">how it works</a>,
		<a href="https://github.com/indexable-inc/ra-jit/blob/main/docs/results.md">full results</a>,
		<a href="https://github.com/indexable-inc/ra-jit/blob/main/docs/support.md">what is supported</a>,
		<a href="https://github.com/indexable-inc/ra-jit">GitHub</a>.
	</p>

	<h2>Credits</h2>
	<p class="dim">
		ra-jit stands on the shoulders of giants. Thank you to the people who built
		<a href="https://github.com/rust-lang/rust-analyzer">rust-analyzer</a>,
		<a href="https://cranelift.dev">Cranelift</a>,
		<a href="https://llvm.org">LLVM</a> and <a href="https://github.com/TheDan64/inkwell">inkwell</a>,
		<a href="https://github.com/salsa-rs/salsa">salsa</a>,
		<a href="https://github.com/gimli-rs/object">object</a>,
		<a href="https://github.com/rust-lang/rust">Rust</a>, and
		<a href="https://github.com/DioxusLabs/rustd">rustd</a> (DioxusLabs), whose code ra-jit includes.
		None of these projects endorses ra-jit.
		Made at <a href="https://ix.dev">ix</a> for den's native app. MIT or Apache-2.0.
	</p>
</main>

<style>
	main { max-width: 720px; margin: 0 auto; padding: 64px 20px 96px; }
	h1 { margin: 0 0 0.2em; font-size: 44px; font-weight: 700; letter-spacing: -0.03em; color: var(--ink-1); }
	.lede { font-size: 20px; line-height: 1.45; color: var(--ink-1); }
	img { display: block; width: 100%; height: auto; margin: 0 0 1.5em; border-radius: 12px; box-shadow: inset 0 0 0 1px var(--line); background: var(--code-bg); }
</style>
