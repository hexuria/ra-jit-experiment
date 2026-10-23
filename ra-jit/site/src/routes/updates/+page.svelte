<script lang="ts">
	import { enhance } from '$app/forms';
	import type { PageProps } from './$types';

	let { data, form }: PageProps = $props();
	let sending = $state(false);
</script>

<svelte:head>
	<title>Tell us you use ra-jit</title>
	<meta name="description" content="Leave your email and why you want to use ra-jit." />
	{#if data.siteKey}
		<script src="https://challenges.cloudflare.com/turnstile/v0/api.js" async defer></script>
	{/if}
</svelte:head>

<main class="doc">
	<p class="back"><a href="/">ra-jit</a></p>
	<h1>Tell us you use ra-jit</h1>

	{#if form?.ok}
		<p class="lede">Thanks, got it.</p>
		<p class="dim"><a href="/">Back to ra-jit</a></p>
	{:else}
		<p class="lede">Leave your email and why you want it. It helps us see who uses ra-jit.</p>

		<form
			method="POST"
			use:enhance={() => {
				sending = true;
				return async ({ update }) => {
					await update({ reset: false });
					sending = false;
					(window as { turnstile?: { reset: () => void } }).turnstile?.reset();
				};
			}}
		>
			<input type="hidden" name="ref" value={data.ref} />
			<label>
				<span>Email</span>
				<input name="email" type="email" required maxlength="254" autocomplete="email" value={form?.email ?? ''} />
			</label>
			<label>
				<span>Company <em>Optional</em></span>
				<input name="company" type="text" maxlength="200" autocomplete="organization" value={form?.company ?? ''} />
			</label>
			<label>
				<span>Why do you want to use ra-jit? <em>Optional</em></span>
				<textarea name="why" rows="3" maxlength="2000">{form?.why ?? ''}</textarea>
			</label>
			{#if data.siteKey}
				<div class="cf-turnstile" data-sitekey={data.siteKey} data-theme="dark"></div>
			{/if}
			{#if form?.error}<p class="error" role="alert">{form.error}</p>{/if}
			<button type="submit" disabled={sending}>{sending ? 'Sending…' : 'Send'}</button>
		</form>
	{/if}

	<p class="dim privacy">We keep your email and answers to see who uses ra-jit, and may write to you about it. We don't share them.</p>
</main>

<style>
	main { max-width: 560px; margin: 0 auto; padding: 64px 20px 96px; }
	.back { margin: 0 0 1.5em; font-size: 14px; }
	h1 { margin: 0 0 0.2em; font-size: 44px; font-weight: 700; letter-spacing: -0.03em; color: var(--ink-1); }
	.lede { font-size: 20px; line-height: 1.45; color: var(--ink-1); }
	form { display: flex; flex-direction: column; gap: 18px; margin: 1.5em 0 2em; }
	label { display: flex; flex-direction: column; gap: 6px; }
	label span { font-size: 14px; font-weight: 500; color: var(--ink-1); }
	label em { font-style: normal; font-weight: 400; color: var(--ink-3); }
	input, textarea {
		width: 100%;
		padding: 10px 14px;
		border: 0;
		border-radius: 10px;
		background: var(--code-bg);
		box-shadow: inset 0 0 0 1px var(--line);
		color: var(--ink-1);
		font: inherit;
		font-size: 15px;
		resize: vertical;
	}
	input:focus, textarea:focus { outline: none; box-shadow: inset 0 0 0 1px var(--ink-3); }
	button {
		align-self: flex-start;
		padding: 10px 18px;
		border: 0;
		border-radius: 10px;
		background: var(--ink-1);
		color: var(--bg);
		font: inherit;
		font-size: 15px;
		font-weight: 600;
		cursor: pointer;
	}
	button:hover { background: var(--g-12); }
	button:disabled { opacity: 0.6; cursor: default; }
	.error { margin: 0; color: var(--danger); font-size: 14px; }
	.privacy { font-size: 14px; }
</style>
