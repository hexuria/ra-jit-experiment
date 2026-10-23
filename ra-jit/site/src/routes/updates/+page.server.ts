import { fail } from '@sveltejs/kit';
import type { Actions, PageServerLoad } from './$types';

// Server-rendered: it reads the Turnstile site key at runtime and stores signups in D1.
export const prerender = false;

const EMAIL_MAX = 254;
const COMPANY_MAX = 200;
const WHY_MAX = 2000;
const META_MAX = 512;
// Rate limit, counted over the last minute of POSTs in D1 (no IPs are kept, so the second cap is global).
const PER_EMAIL_PER_MINUTE = 3;
const ALL_PER_MINUTE = 60;
const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const UTM = ['utm_source', 'utm_medium', 'utm_campaign', 'utm_term', 'utm_content'] as const;

export const load: PageServerLoad = ({ platform, request }) => ({
	siteKey: platform?.env.TURNSTILE_SITE_KEY || '',
	// Where the visitor came from before /updates; the POST's own Referer is /updates itself.
	ref: request.headers.get('referer') ?? ''
});

function text(v: FormDataEntryValue | string | null | undefined, max: number): string | null {
	if (typeof v !== 'string') return null;
	const t = v.trim();
	return t ? t.slice(0, max) : null;
}

async function turnstileOk(secret: string, token: string, ip: string | null): Promise<boolean> {
	const body = new FormData();
	body.append('secret', secret);
	body.append('response', token);
	if (ip) body.append('remoteip', ip); // sent to Turnstile only, never stored
	const res = await fetch('https://challenges.cloudflare.com/turnstile/v0/siteverify', { method: 'POST', body });
	if (!res.ok) return false;
	const out: { success?: boolean } = await res.json();
	return out.success === true;
}

export const actions: Actions = {
	default: async ({ request, platform, url }) => {
		const env = platform?.env;
		const data = await request.formData();
		const entry = data.get('email');
		const raw = typeof entry === 'string' ? entry : '';
		const email = raw.trim().toLowerCase();
		const company = text(data.get('company'), COMPANY_MAX);
		const why = text(data.get('why'), WHY_MAX);
		const values = { email: raw, company: company ?? '', why: why ?? '' };

		if (!env?.DB) return fail(500, { ...values, error: 'Signups are not set up on this server.' });

		if (!email || email.length > EMAIL_MAX || !EMAIL_RE.test(email)) {
			return fail(400, { ...values, error: 'Enter a valid email address.' });
		}

		if (env.TURNSTILE_SECRET_KEY) {
			const token = data.get('cf-turnstile-response');
			const ok =
				typeof token === 'string' &&
				token !== '' &&
				(await turnstileOk(env.TURNSTILE_SECRET_KEY, token, request.headers.get('cf-connecting-ip')));
			if (!ok) return fail(400, { ...values, error: 'The check did not pass. Try again.' });
		}

		const now = Math.floor(Date.now() / 1000);
		const counts = await env.DB.prepare(
			`SELECT
				(SELECT count(*) FROM signup_attempts WHERE email = ?1 AND at > ?2) AS mine,
				(SELECT count(*) FROM signup_attempts WHERE at > ?2) AS everyone`
		)
			.bind(email, now - 60)
			.first<{ mine: number; everyone: number }>();
		if (counts && (counts.mine >= PER_EMAIL_PER_MINUTE || counts.everyone >= ALL_PER_MINUTE)) {
			return fail(429, { ...values, error: 'Too many signups right now. Try again in a minute.' });
		}

		const cf = platform?.cf;
		const h = request.headers;
		const q = url.searchParams;
		const meta = {
			country: text(cf?.country as string | undefined, META_MAX),
			region: text(cf?.region as string | undefined, META_MAX),
			city: text(cf?.city as string | undefined, META_MAX),
			asn: typeof cf?.asn === 'number' ? cf.asn : null,
			as_organization: text(cf?.asOrganization as string | undefined, META_MAX),
			timezone: text(cf?.timezone as string | undefined, META_MAX),
			user_agent: text(h.get('user-agent'), META_MAX),
			referer: text(data.get('ref'), META_MAX) ?? text(h.get('referer'), META_MAX),
			accept_language: text(h.get('accept-language'), META_MAX),
			src: text(q.get('src'), META_MAX),
			...Object.fromEntries(UTM.map((k) => [k, text(q.get(k), META_MAX)]))
		};
		const cols = Object.keys(meta);

		// A repeat signup keeps its first-touch metadata and updates the answers it gave this time.
		await env.DB.batch([
			env.DB.prepare('INSERT INTO signup_attempts (email, at) VALUES (?, ?)').bind(email, now),
			env.DB.prepare('DELETE FROM signup_attempts WHERE at < ?').bind(now - 86400),
			env.DB.prepare(
				`INSERT INTO signups (email, company, why, ${cols.join(', ')})
				VALUES (?, ?, ?, ${cols.map(() => '?').join(', ')})
				ON CONFLICT (email) DO UPDATE SET
					company = coalesce(excluded.company, signups.company),
					why = coalesce(excluded.why, signups.why),
					updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')`
			).bind(email, company, why, ...Object.values(meta))
		]);

		return { ok: true };
	}
};
