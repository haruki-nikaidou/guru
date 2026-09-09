import type { RoleName } from '#lib/dto/identity.js';

/** Mirrors `modules/auth/src/utils/rbac.rs`; the backend stays authoritative. */
export const canEditWorkspace = (role: RoleName) => role === 'admin' || role === 'maintainer';
export const canManageApiKeys = (role: RoleName) => role === 'admin' || role === 'maintainer';
export const canManageAccounts = (role: RoleName) => role === 'admin';
