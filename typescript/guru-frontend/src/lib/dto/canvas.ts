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

/** A canvas as the switcher lists it; `isRoot` is false for a subcanvas. */
export type CanvasOptionEntry = CanvasOption & { isRoot: boolean };

export type CanvasSummary = CanvasOption & {
	/** null when the per-canvas detail/validate fan-out failed for this canvas */
	stats: { servers: number; nodes: number; edges: number } | null;
	/** null on the same failure; `problems` is capped at 5 entries. */
	health: { errors: number; warnings: number; problems: CanvasProblem[] } | null;
	/** The canvas whose import node embeds this one; null for a root. */
	parent: CanvasOption | null;
};

/** One canvas of a nesting tree, as `GetCanvasTree` returns it. */
export type CanvasTreeNode = {
	canvas: CanvasOption;
	children: CanvasTreeNode[];
};

/**
 * A canvas tree plus the path from its root down to the canvas that was asked
 * about, root first and that canvas last.
 */
export type CanvasTrail = {
	root: CanvasTreeNode;
	path: CanvasOption[];
};
