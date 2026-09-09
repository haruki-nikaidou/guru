import { defineEnvVars } from '@sveltejs/kit/env';

export const variables = defineEnvVars({
	GURU_GRPC_URL: {
		description: "Dashboard gRPC endpoint of bin/guru-master (mode dashboard_grpc)'s h2c listener",
		schema: value => value ?? '127.0.0.1:50051'
	}
});
