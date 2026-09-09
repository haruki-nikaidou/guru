import { m } from '#lib/paraglide/messages.js';
import { getLocale } from '#lib/paraglide/runtime.js';

/** Backend timestamps are RFC3339 with nanoseconds, or `''` when absent. */
export function formatTimestamp(rfc3339: string): string {
	if (rfc3339 === '') return m.common_never();
	const parsed = new Date(rfc3339);
	return Number.isNaN(parsed.getTime()) ? rfc3339 : parsed.toLocaleString(getLocale());
}

export function roleLabel(role: string): string {
	switch (role) {
		case 'admin':
			return m.role_admin();
		case 'maintainer':
			return m.role_maintainer();
		case 'observer':
			return m.role_observer();
		default:
			return m.role_unknown();
	}
}
