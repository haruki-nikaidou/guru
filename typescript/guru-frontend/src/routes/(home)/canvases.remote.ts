import { ProblemKind, ProblemSeverity } from 'app-protobuf/orchestration/orchestration';
import * as v from 'valibot';
import type { CanvasOption, CanvasProblem, CanvasSummary } from '#lib/dto/canvas.js';
import { callGrpc } from '#lib/server/errors.js';
import { orchestrationClient } from '#lib/server/grpc.js';
import { requireSessionId, sessionMetadata } from '#lib/server/session.js';
import { command, form, query } from '$app/server';

// The control plane only rejects empty names; every other limit below is this
// dashboard's own.
const nameSchema = v.pipe(
	v.string(),
	v.trim(),
	v.minLength(1, 'canvas_name_required'),
	v.maxLength(128, 'canvas_name_too_long')
);
const descriptionSchema = v.optional(
	v.pipe(v.string(), v.trim(), v.maxLength(1000, 'canvas_description_too_long')),
	''
);
const idSchema = v.pipe(v.string(), v.minLength(1, 'id_required'));

const PROBLEM_LIMIT = 5;

function toProblem(severity: ProblemSeverity, kind: ProblemKind, message: string): CanvasProblem {
	return {
		severity:
			severity === ProblemSeverity.PROBLEM_ERROR
				? 'error'
				: severity === ProblemSeverity.PROBLEM_WARNING
					? 'warning'
					: 'unknown',
		kind: ProblemKind[kind] ?? 'UNSPECIFIED',
		message
	};
}

/**
 * Names only: what the editor's canvas switcher and settings page need. Kept
 * apart from `listCanvases` so opening a canvas does not pay for the per-canvas
 * detail/validate fan-out behind the dashboard cards.
 */
export const listCanvasOptions = query(async (): Promise<CanvasOption[]> => {
	const metadata = sessionMetadata(requireSessionId());
	const { canvases } = await callGrpc(() => orchestrationClient().listCanvases({}, { metadata }));
	return canvases
		.map(canvas => ({ id: canvas.id, name: canvas.name, description: canvas.description }))
		.sort((a, b) => a.name.localeCompare(b.name));
});

export const listCanvases = query(async (): Promise<CanvasSummary[]> => {
	const metadata = sessionMetadata(requireSessionId());

	const { canvases } = await callGrpc(() => orchestrationClient().listCanvases({}, { metadata }));
	// The backend returns no ordering and no timestamps to sort by.
	const ordered = [...canvases].sort((a, b) => a.name.localeCompare(b.name));

	return Promise.all(
		ordered.map(async (canvas): Promise<CanvasSummary> => {
			const base = { id: canvas.id, name: canvas.name, description: canvas.description };
			try {
				// Deliberately outside `callGrpc`: a canvas deleted between the list and
				// this fan-out must degrade one card, not 404 the whole page.
				const [detail, validation] = await Promise.all([
					orchestrationClient().getCanvas({ canvasId: canvas.id }, { metadata }),
					orchestrationClient().validateCanvas({ canvasId: canvas.id }, { metadata })
				]);

				let errors = 0;
				let warnings = 0;
				for (const problem of validation.problems) {
					if (problem.severity === ProblemSeverity.PROBLEM_ERROR) errors += 1;
					else if (problem.severity === ProblemSeverity.PROBLEM_WARNING) warnings += 1;
				}

				return {
					...base,
					stats: {
						servers: detail.servers.length,
						nodes: detail.nodes.length,
						edges: detail.edges.length
					},
					health: {
						errors,
						warnings,
						problems: validation.problems
							.slice(0, PROBLEM_LIMIT)
							.map(problem => toProblem(problem.severity, problem.kind, problem.message))
					}
				};
			} catch {
				return { ...base, stats: null, health: null };
			}
		})
	);
});

export const createCanvas = form(
	v.object({ name: nameSchema, description: descriptionSchema }),
	async ({ name, description }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() => orchestrationClient().createCanvas({ name, description }, { metadata }));
		await Promise.all([listCanvases().refresh(), listCanvasOptions().refresh()]);
		return { ok: true as const };
	}
);

export const updateCanvas = form(
	v.object({ canvasId: idSchema, name: nameSchema, description: descriptionSchema }),
	async ({ canvasId, name, description }) => {
		const metadata = sessionMetadata(requireSessionId());
		// UpdateCanvas is a full replace: omitting the description clears it.
		await callGrpc(() =>
			orchestrationClient().updateCanvas({ canvasId, name, description }, { metadata })
		);
		await Promise.all([listCanvases().refresh(), listCanvasOptions().refresh()]);
		return { ok: true as const };
	}
);

export const deleteCanvas = command(v.object({ canvasId: idSchema }), async ({ canvasId }) => {
	const metadata = sessionMetadata(requireSessionId());
	await callGrpc(() => orchestrationClient().deleteCanvas({ canvasId }, { metadata }));
	await Promise.all([listCanvases().refresh(), listCanvasOptions().refresh()]);
	return { ok: true as const };
});
