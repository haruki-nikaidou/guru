import { type Identity, toRoleName } from '#lib/dto/identity.js';
import { callGrpc } from './errors.js';
import { authClient } from './grpc.js';
import { sessionMetadata } from './session.js';

export async function fetchIdentity(sessionId: string): Promise<Identity> {
	const reply = await callGrpc(() =>
		authClient().currentIdentity({}, { metadata: sessionMetadata(sessionId) })
	);
	return { accountId: reply.accountId, email: reply.email, role: toRoleName(reply.role) };
}
