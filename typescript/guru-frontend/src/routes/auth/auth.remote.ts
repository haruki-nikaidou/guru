import { redirect } from '@sveltejs/kit';
import { LoginResult } from 'app-protobuf/auth/auth';
import * as v from 'valibot';
import { callGrpc } from '#lib/server/errors.js';
import { authClient } from '#lib/server/grpc.js';
import {
	clearSessionCookie,
	SESSION_COOKIE,
	sessionMetadata,
	setSessionCookie
} from '#lib/server/session.js';
import { command, form, getRequestEvent } from '$app/server';

const loginSchema = v.object({
	email: v.pipe(
		v.string(),
		v.trim(),
		v.toLowerCase(),
		v.minLength(1, 'email_required'),
		v.email('email_invalid')
	),
	password: v.pipe(v.string(), v.minLength(1, 'password_required')),
	next: v.optional(v.string(), '/')
});

export const login = form(loginSchema, async ({ email, password, next }) => {
	const { request } = getRequestEvent();
	const userAgent = request.headers.get('user-agent') ?? '';

	// Login is the only unauthenticated RPC: it carries no session metadata.
	const reply = await callGrpc(() => authClient().login({ email, password, userAgent }));

	if (reply.result !== LoginResult.SUCCESS) {
		// The backend's login path is constant-time and never reveals whether the
		// email exists, so neither does this message.
		return { error: 'invalid_credentials' as const };
	}

	setSessionCookie(reply.sessionId);
	redirect(303, next.startsWith('/') && !next.startsWith('//') ? next : '/');
});

export const logout = command(async () => {
	const sessionId = getRequestEvent().cookies.get(SESSION_COOKIE);
	if (sessionId) {
		try {
			await authClient().logout({}, { metadata: sessionMetadata(sessionId) });
		} catch {
			// Logout is idempotent server-side; a failure must not trap the client.
		}
	}
	clearSessionCookie();
	return { ok: true as const };
});
