import { getContext, setContext } from 'svelte';

/**
 * Which node the side panel is editing, published to the node components so the
 * card can draw a focus ring. It travels through context rather than node data:
 * the flow mirror is reconciled against the server graph, and folding a purely
 * local UI flag into it would re-render (and re-measure) nodes on every click.
 */
export type FocusedNode = {
	/** Flow node id of the panel's target, or `null` when the panel is closed. */
	readonly current: string | null;
};

const KEY = Symbol.for('canvas-focused-node');

const NONE: FocusedNode = { current: null };

export function setFocusedNode(focused: FocusedNode): void {
	setContext(KEY, focused);
}

/** Falls back to "nothing focused" for node components rendered outside a flow. */
export function useFocusedNode(): FocusedNode {
	return getContext<FocusedNode | undefined>(KEY) ?? NONE;
}
