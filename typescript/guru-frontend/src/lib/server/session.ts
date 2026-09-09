import { redirect } from '@sveltejs/kit';
import { Metadata } from 'nice-grpc';
import { getRequestEvent } from '$app/server';

export const SESSION_COOKIE = 'guru_session';
/** Mirrors the backend's 7-day sliding idle TTL (`modules/auth/src/config.rs`). */
const MAX_AGE = 60 * 60 * 24 * 7;

export function setSessionCookie(sessionId: string): void {
	const { cookies, url } = getRequestEvent();
	cookies.set(SESSION_COOKIE, sessionId, {
		path: '/',
		httpOnly: true,
		sameSite: 'lax',
		secure: url.protocol === 'https:',
		maxAge: MAX_AGE
	});
}

export function clearSessionCookie(): void {
	getRequestEvent().cookies.delete(SESSION_COOKIE, { path: '/' });
}

/** The control plane authenticates dashboard calls with the raw session id. */
export function sessionMetadata(sessionId: string): Metadata {
	return new Metadata({ 'x-session-id': sessionId });
}

/** Returns the caller's session id, or redirects to the login page. */
export function requireSessionId(): string {
	const sessionId = getRequestEvent().cookies.get(SESSION_COOKIE);
	if (!sessionId) redirect(303, '/auth');
	return sessionId;
}
