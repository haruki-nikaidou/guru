import type { AccountRow, AssignableRole } from '#lib/dto/identity.js';

export type AccountMutation = { kind: 'set_role'; role: AssignableRole } | { kind: 'delete' };

/**
 * Guards the backend deliberately does not implement: it has no self-demotion,
 * self-deletion or last-admin protection. Shared by the remote functions (which
 * are authoritative) and the panels (which only disable the affordance), so the
 * two can never disagree.
 *
 * Returns the `App.Error` code that blocks the mutation, or null when allowed.
 */
export function accountMutationBlock(
	callerId: string,
	accounts: AccountRow[],
	targetId: string,
	mutation: AccountMutation
): 'cannot_modify_self' | 'last_admin' | null {
	if (targetId === callerId) return 'cannot_modify_self';

	const target = accounts.find(account => account.id === targetId);
	// Unknown target: the backend's delete is idempotent and role updates 404.
	if (target?.role !== 'admin') return null;

	const admins = accounts.filter(account => account.role === 'admin').length;
	const losesAdmin = mutation.kind === 'delete' || mutation.role !== 'admin';
	return admins <= 1 && losesAdmin ? 'last_admin' : null;
}
