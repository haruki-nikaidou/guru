import { error, redirect } from '@sveltejs/kit';
import { ClientError, Status } from 'nice-grpc';
import { clearSessionCookie } from './session.js';

/**
 * The single gRPC error boundary: wraps one client call and turns transport /
 * authz failures into SvelteKit errors carrying a stable `App.Error` code.
 *
 * Only the client call itself belongs inside `fn` — redirects and errors thrown
 * by surrounding code must not be swallowed here.
 */
export async function callGrpc<T>(fn: () => Promise<T>): Promise<T> {
	try {
		return await fn();
	} catch (err) {
		if (err instanceof ClientError) {
			switch (err.code) {
				case Status.UNAUTHENTICATED:
					clearSessionCookie();
					throw redirect(303, '/auth');
				case Status.PERMISSION_DENIED:
					throw error(403, { message: 'Forbidden', code: 'forbidden' });
				case Status.NOT_FOUND:
					throw error(404, { message: 'Not found', code: 'not_found' });
				case Status.INVALID_ARGUMENT:
				case Status.FAILED_PRECONDITION:
					// These carry actionable English text from the control plane.
					throw error(400, { message: err.details, code: 'server_message' });
			}
		}
		console.error(err);
		throw error(500, { message: 'Internal server error', code: 'internal' });
	}
}
