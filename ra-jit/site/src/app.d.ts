import type { D1Database, ExecutionContext, IncomingRequestCfProperties } from '@cloudflare/workers-types';

declare global {
	namespace App {
		interface Platform {
			env: {
				DB: D1Database;
				TURNSTILE_SITE_KEY?: string;
				TURNSTILE_SECRET_KEY?: string;
			};
			cf?: IncomingRequestCfProperties;
			ctx: ExecutionContext;
		}
	}
}

export {};
