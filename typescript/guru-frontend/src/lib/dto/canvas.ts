export type CanvasProblem = {
	severity: 'error' | 'warning' | 'unknown';
	/** `ProblemKind` name, for grouping only — always display `message`. */
	kind: string;
	message: string;
};

export type CanvasSummary = {
	id: string;
	name: string;
	description: string;
	/** null when the per-canvas detail/validate fan-out failed for this canvas */
	stats: { servers: number; nodes: number; edges: number } | null;
	/** null on the same failure; `problems` is capped at 5 entries. */
	health: { errors: number; warnings: number; problems: CanvasProblem[] } | null;
};
