import { m } from '#lib/paraglide/messages.js';

/**
 * Server code never calls `m.*`: remote functions emit these stable codes and
 * the client translates them. Every lookup is an explicit switch so paraglide
 * can tree-shake per message.
 */

/** Valibot issue codes, used as the `message` argument of every validation rule. */
export function issueMessage(code: string): string {
	switch (code) {
		case 'canvas_name_required':
			return m.issue_canvas_name_required();
		case 'canvas_name_too_long':
			return m.issue_canvas_name_too_long();
		case 'canvas_description_too_long':
			return m.issue_canvas_description_too_long();
		case 'email_required':
			return m.issue_email_required();
		case 'email_invalid':
			return m.issue_email_invalid();
		case 'password_required':
			return m.issue_password_required();
		case 'password_too_short':
			return m.issue_password_too_short();
		case 'password_mismatch':
			return m.issue_password_mismatch();
		case 'api_key_name_required':
			return m.issue_api_key_name_required();
		case 'api_key_name_too_long':
			return m.issue_api_key_name_too_long();
		case 'role_invalid':
			return m.issue_role_invalid();
		case 'id_required':
			return m.issue_id_required();
		default:
			return code;
	}
}

/**
 * `App.Error` codes. The special code `server_message` means the control plane
 * supplied actionable English text, which is displayed verbatim.
 */
export function errorMessage(code: string | undefined, fallback: string): string {
	switch (code) {
		case 'forbidden':
			return m.error_forbidden();
		case 'not_found':
			return m.error_not_found();
		case 'internal':
			return m.error_internal();
		case 'cannot_modify_self':
			return m.error_cannot_modify_self();
		case 'last_admin':
			return m.error_last_admin();
		default:
			return fallback.length > 0 ? fallback : m.error_internal();
	}
}

/** Domain result codes carried by the reply payload enums. */
export function resultMessage(code: string): string {
	switch (code) {
		case 'invalid_credentials':
			return m.result_invalid_credentials();
		case 'email_taken':
			return m.result_email_taken();
		case 'wrong_password':
			return m.result_wrong_password();
		default:
			return m.error_internal();
	}
}
