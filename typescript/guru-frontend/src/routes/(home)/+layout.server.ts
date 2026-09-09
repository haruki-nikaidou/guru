import { redirect } from '@sveltejs/kit';
import { fetchIdentity } from '#lib/server/identity.js';
import { SESSION_COOKIE } from '#lib/server/session.js';
import type { LayoutServerLoad } from './$types.js';

export const load: LayoutServerLoad = async ({ cookies, url }) => {
	const sessionId = cookies.get(SESSION_COOKIE);
	if (!sessionId) redirect(303, `/auth?next=${encodeURIComponent(url.pathname)}`);

	// An expired session surfaces as UNAUTHENTICATED, which `callGrpc` turns into
	// a cookie clear plus the same redirect.
	return { identity: await fetchIdentity(sessionId) };
};
