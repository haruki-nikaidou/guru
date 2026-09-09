import { Role } from 'app-protobuf/auth/auth';

export type RoleName = 'admin' | 'maintainer' | 'observer' | 'unknown';
export type AssignableRole = Exclude<RoleName, 'unknown'>;

export const ASSIGNABLE_ROLES: AssignableRole[] = ['admin', 'maintainer', 'observer'];

export type Identity = { accountId: string; email: string; role: RoleName };
export type AccountRow = { id: string; email: string; role: RoleName };
/** `createdAt` is the backend's RFC3339 string, or `''` when absent. */
export type ApiKeyRow = { id: string; name: string; createdAt: string };

export function toRoleName(role: Role): RoleName {
	switch (role) {
		case Role.ADMIN:
			return 'admin';
		case Role.MAINTAINER:
			return 'maintainer';
		case Role.OBSERVER:
			return 'observer';
		default:
			return 'unknown';
	}
}

/** Never emits `Role.UNSPECIFIED`, which the backend rejects with INVALID_ARGUMENT. */
export function fromRoleName(role: AssignableRole): Role {
	switch (role) {
		case 'admin':
			return Role.ADMIN;
		case 'maintainer':
			return Role.MAINTAINER;
		case 'observer':
			return Role.OBSERVER;
	}
}
