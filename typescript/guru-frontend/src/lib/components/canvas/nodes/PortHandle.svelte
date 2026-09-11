<script lang="ts">
import { Handle, Position } from '@xyflow/svelte';
import type { CanvasPort } from '#lib/dto/topology.js';

// The one place a Svelte Flow handle is created. Output ports are handles on the
// left, input ports on the right, and the dot takes the port *kind*'s colour.
let { port, label }: { port: CanvasPort; label: string } = $props();

const colour = $derived(
	port.kind === 'derive_listen' ? 'var(--canvas-port-listen)' : 'var(--canvas-port-destination)'
);
const output = $derived(port.direction === 'output');
</script>

<!-- `relative` anchors the handle to this row, giving one handle per port line. -->
<div class="relative flex items-center px-3 py-1 text-xs" class:justify-end={!output}>
	<Handle
		id={port.id}
		type={output ? 'source' : 'target'}
		position={output ? Position.Left : Position.Right}
		style="background:{colour}; border-color:{colour}; width:10px; height:10px;"
	/>
	<span class="font-mono text-muted-foreground">{label}</span>
</div>
