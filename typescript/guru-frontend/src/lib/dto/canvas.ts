export type CanvasProblem = {
	severity: 'error' | 'warning' | 'unknown';
	/** `ProblemKind` name, for grouping only — always display `message`. */
	kind: string;
	message: string;
};

/** The identity of a canvas: what the switcher and settings page work with. */
export type CanvasOption = {
	id: string;
	name: string;
	description: string;
};

export type CanvasSummary = CanvasOption & {
	/** null when the per-canvas detail/validate fan-out failed for this canvas */
	stats: { servers: number; nodes: number; edges: number } | null;
	/** null on the same failure; `problems` is capped at 5 entries. */
	health: { errors: number; warnings: number; problems: CanvasProblem[] } | null;
};
