/**
 * The canvas editor's remote functions live with their route (SvelteKit requires
 * `*.remote.ts` under `src`), while the components live in `#lib`. This module is
 * the single place that bridges the two, so no component carries a deep relative
 * path.
 */
export * from '../../../routes/(home)/canvas/[canvasId]/topology.remote.js';
