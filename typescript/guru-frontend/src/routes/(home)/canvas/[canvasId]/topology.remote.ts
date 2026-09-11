import type {
	Node as ProtoNode,
	Server as ProtoServer
} from 'app-protobuf/orchestration/orchestration';
import {
	Ipv6Resolve,
	LoadBalanceMode,
	PortDirection,
	PortKind,
	ProblemKind,
	ProblemSeverity,
	ProxyProtocolVersion,
	RelayProtocol
} from 'app-protobuf/orchestration/orchestration';
import * as v from 'valibot';
import type {
	CanvasGraph,
	CanvasPort,
	Ipv6ResolveName,
	LoadBalanceModeName,
	PodDto,
	PortDirectionName,
	PortKindName,
	ProxyProtocolName,
	RelayProtocolName,
	ServerDto,
	StandaloneNode,
	TopologyProblem
} from '#lib/dto/topology.js';
import { callGrpc } from '#lib/server/errors.js';
import { orchestrationClient } from '#lib/server/grpc.js';
import { requireSessionId, sessionMetadata } from '#lib/server/session.js';
import { command, query } from '$app/server';

// The control plane is authoritative on permissions: no role check happens here.
const idSchema = v.pipe(v.string(), v.minLength(1, 'id_required'));
const nameSchema = v.pipe(
	v.string(),
	v.trim(),
	v.minLength(1, 'node_name_required'),
	v.maxLength(128, 'node_name_too_long')
);
const commentSchema = v.optional(
	v.pipe(v.string(), v.trim(), v.maxLength(1000, 'node_comment_too_long')),
	''
);
const coordSchema = v.pipe(v.number(), v.integer(), v.safeInteger());
const portSchema = v.pipe(
	v.number(),
	v.integer(),
	v.minValue(1, 'port_out_of_range'),
	v.maxValue(65535, 'port_out_of_range')
);
const memberCountSchema = v.pipe(
	v.number(),
	v.integer(),
	v.minValue(2, 'member_count_out_of_range'),
	v.maxValue(256, 'member_count_out_of_range')
);
const ipSchema = v.pipe(v.string(), v.trim(), v.minLength(1, 'ip_required'));
const logLevelSchema = v.pipe(v.string(), v.trim(), v.minLength(1, 'log_level_required'));
const proxySchema = v.picklist(['none', 'v1', 'v2'] as const);
const relayProtocolSchema = v.picklist(['tcp_raw', 'tcp_tls', 'quic'] as const);
const balanceModeSchema = v.picklist(['round_robin', 'random', 'ip_hash', 'fallback'] as const);
const ipv6Schema = v.picklist(['required', 'preferred', 'tolerated', 'forbidden'] as const);
const tlsSchema = v.nullable(
	v.object({
		sni: v.string(),
		dnsProviderId: v.string(),
		domainId: v.string(),
		acmeDirectory: v.string()
	})
);

// Every unknown / UNSPECIFIED value decodes to the backend's own default.
function toProxy(value: ProxyProtocolVersion): ProxyProtocolName {
	switch (value) {
		case ProxyProtocolVersion.PROXY_V1:
			return 'v1';
		case ProxyProtocolVersion.PROXY_V2:
			return 'v2';
		default:
			return 'none';
	}
}
function fromProxy(value: ProxyProtocolName): ProxyProtocolVersion {
	switch (value) {
		case 'v1':
			return ProxyProtocolVersion.PROXY_V1;
		case 'v2':
			return ProxyProtocolVersion.PROXY_V2;
		default:
			return ProxyProtocolVersion.UNSPECIFIED;
	}
}
function toRelayProtocol(value: RelayProtocol): RelayProtocolName {
	switch (value) {
		case RelayProtocol.RELAY_TCP_TLS:
			return 'tcp_tls';
		case RelayProtocol.RELAY_QUIC:
			return 'quic';
		default:
			return 'tcp_raw';
	}
}
function fromRelayProtocol(value: RelayProtocolName): RelayProtocol {
	switch (value) {
		case 'tcp_tls':
			return RelayProtocol.RELAY_TCP_TLS;
		case 'quic':
			return RelayProtocol.RELAY_QUIC;
		default:
			return RelayProtocol.RELAY_TCP_RAW;
	}
}
function toBalanceMode(value: LoadBalanceMode): LoadBalanceModeName {
	switch (value) {
		case LoadBalanceMode.RANDOM:
			return 'random';
		case LoadBalanceMode.IP_HASH:
			return 'ip_hash';
		case LoadBalanceMode.FALLBACK:
			return 'fallback';
		default:
			return 'round_robin';
	}
}
function fromBalanceMode(value: LoadBalanceModeName): LoadBalanceMode {
	switch (value) {
		case 'random':
			return LoadBalanceMode.RANDOM;
		case 'ip_hash':
			return LoadBalanceMode.IP_HASH;
		case 'fallback':
			return LoadBalanceMode.FALLBACK;
		default:
			return LoadBalanceMode.ROUND_ROBIN;
	}
}
function toIpv6(value: Ipv6Resolve): Ipv6ResolveName {
	switch (value) {
		case Ipv6Resolve.IPV6_REQUIRED:
			return 'required';
		case Ipv6Resolve.IPV6_PREFERRED:
			return 'preferred';
		case Ipv6Resolve.IPV6_FORBIDDEN:
			return 'forbidden';
		default:
			// `IPV6_RESOLVE_UNSPECIFIED` decodes to `Tolerated` in the control plane.
			return 'tolerated';
	}
}
function fromIpv6(value: Ipv6ResolveName): Ipv6Resolve {
	switch (value) {
		case 'required':
			return Ipv6Resolve.IPV6_REQUIRED;
		case 'preferred':
			return Ipv6Resolve.IPV6_PREFERRED;
		case 'forbidden':
			return Ipv6Resolve.IPV6_FORBIDDEN;
		default:
			return Ipv6Resolve.IPV6_TOLERATED;
	}
}
const toPortKind = (value: PortKind): PortKindName =>
	value === PortKind.DERIVE_LISTEN ? 'derive_listen' : 'derive_destination';
const toPortDirection = (value: PortDirection): PortDirectionName =>
	value === PortDirection.PORT_OUTPUT ? 'output' : 'input';

const toPorts = (node: ProtoNode): CanvasPort[] =>
	node.ports
		.map(port => ({
			id: port.id,
			kind: toPortKind(port.kind),
			direction: toPortDirection(port.direction),
			key: port.key,
			position: Number(port.position)
		}))
		.sort((a, b) => a.position - b.position);

function toProblem(problem: {
	severity: ProblemSeverity;
	kind: ProblemKind;
	message: string;
	nodeIds: string[];
	edgeIds: string[];
	portIds: string[];
}): TopologyProblem {
	return {
		severity:
			problem.severity === ProblemSeverity.PROBLEM_ERROR
				? 'error'
				: problem.severity === ProblemSeverity.PROBLEM_WARNING
					? 'warning'
					: 'unknown',
		kind: ProblemKind[problem.kind] ?? 'UNSPECIFIED',
		message: problem.message,
		nodeIds: problem.nodeIds,
		edgeIds: problem.edgeIds,
		portIds: problem.portIds
	};
}

const toPod = (node: ProtoNode, ipRecordId: string, port: number): PodDto => ({
	id: node.id,
	name: node.name,
	comment: node.comment,
	ipRecordId,
	port,
	ports: toPorts(node)
});

/** A standalone node, or `null` for a pod / an unsupported spec. */
function toStandalone(node: ProtoNode): StandaloneNode | null {
	const spec = node.spec;
	const base = {
		id: node.id,
		name: node.name,
		comment: node.comment,
		x: Number(node.position?.x ?? 0n),
		y: Number(node.position?.y ?? 0n),
		ports: toPorts(node)
	};
	if (spec?.entry) {
		const tls = spec.entry.tls;
		return {
			...base,
			kind: 'entry',
			receiveProxyProtocol: toProxy(spec.entry.receiveProxyProtocol),
			tls: tls
				? {
						sni: tls.sni,
						dnsProviderId: tls.dnsProviderId,
						domainId: tls.domainId,
						acmeDirectory: tls.acmeDirectory
					}
				: null
		};
	}
	if (spec?.relay) {
		return {
			...base,
			kind: 'relay',
			protocol: toRelayProtocol(spec.relay.protocol),
			overrideIpAddress: spec.relay.overrideIpAddress,
			overridePort: spec.relay.overridePort
		};
	}
	if (spec?.exit) {
		return {
			...base,
			kind: 'exit',
			destination: spec.exit.destination,
			passProxyProtocol: toProxy(spec.exit.passProxyProtocol)
		};
	}
	if (spec?.loadBalanceDistribute) {
		return {
			...base,
			kind: 'load_balance',
			mode: 'distribute',
			balanceMode: toBalanceMode(spec.loadBalanceDistribute.mode),
			// Every port but the single `destination` output is a member.
			memberCount: base.ports.filter(port => port.key.startsWith('member_')).length
		};
	}
	if (spec?.loadBalanceAggregate) {
		return {
			...base,
			kind: 'load_balance',
			mode: 'aggregate',
			balanceMode: 'round_robin',
			memberCount: base.ports.filter(port => port.key.startsWith('copy_')).length
		};
	}
	return null;
}

const toServer = (server: ProtoServer, pods: PodDto[]): ServerDto => ({
	id: server.id,
	name: server.name,
	icon: server.icon,
	comment: server.comment,
	x: Number(server.position?.x ?? 0n),
	y: Number(server.position?.y ?? 0n),
	ipv6Resolve: toIpv6(server.ipv6Resolve),
	logLevel: server.logLevel,
	lastSeenAt: server.lastSeenAt,
	ips: server.ips.map(ip => ({ id: ip.id, ip: ip.ip, country: ip.country })),
	pods
});

export const getCanvasGraph = query(
	v.object({ canvasId: idSchema }),
	async ({ canvasId }): Promise<CanvasGraph> => {
		const metadata = sessionMetadata(requireSessionId());

		const [detail, validation] = await callGrpc(() =>
			Promise.all([
				orchestrationClient().getCanvas({ canvasId }, { metadata }),
				orchestrationClient().validateCanvas({ canvasId }, { metadata })
			])
		);

		// A pod binds to a server through its ip record; `Server.ips` comes inline.
		const serverByIpRecord = new Map<string, string>();
		for (const server of detail.servers) {
			for (const ip of server.ips) serverByIpRecord.set(ip.id, server.id);
		}

		const podsByServer = new Map<string, PodDto[]>();
		const orphanPods: PodDto[] = [];
		const nodes: StandaloneNode[] = [];
		for (const node of detail.nodes) {
			const pod = node.spec?.pod;
			if (pod) {
				const dto = toPod(node, pod.ipRecordId, pod.port);
				const serverId = serverByIpRecord.get(pod.ipRecordId);
				if (serverId === undefined) {
					orphanPods.push(dto);
					continue;
				}
				const bucket = podsByServer.get(serverId);
				if (bucket) bucket.push(dto);
				else podsByServer.set(serverId, [dto]);
				continue;
			}
			const standalone = toStandalone(node);
			if (standalone) nodes.push(standalone);
		}

		return {
			canvas: {
				id: detail.canvas?.id ?? canvasId,
				name: detail.canvas?.name ?? '',
				description: detail.canvas?.description ?? ''
			},
			servers: detail.servers.map(server => toServer(server, podsByServer.get(server.id) ?? [])),
			nodes,
			edges: detail.edges.map(edge => ({
				id: edge.id,
				sourcePortId: edge.sourcePortId,
				targetPortId: edge.targetPortId
			})),
			problems: validation.problems.map(toProblem),
			orphanPods
		};
	}
);

export const createServerNode = command(
	v.object({ canvasId: idSchema, name: nameSchema, x: coordSchema, y: coordSchema }),
	async ({ canvasId, name, x, y }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			orchestrationClient().createServer(
				{
					canvasId,
					name,
					icon: '',
					comment: '',
					position: { x, y },
					ipv6Resolve: Ipv6Resolve.IPV6_TOLERATED,
					logLevel: 'info'
				},
				{ metadata }
			)
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const updateServerNode = command(
	v.object({
		canvasId: idSchema,
		serverId: idSchema,
		name: nameSchema,
		icon: v.optional(v.string(), ''),
		comment: commentSchema,
		ipv6Resolve: ipv6Schema,
		logLevel: logLevelSchema
	}),
	async ({ canvasId, serverId, name, icon, comment, ipv6Resolve, logLevel }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			orchestrationClient().updateServer(
				{ serverId, name, icon, comment, ipv6Resolve: fromIpv6(ipv6Resolve), logLevel },
				{ metadata }
			)
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

/** Deliberately does not refresh: the dragged position already matches locally. */
export const moveServerNode = command(
	v.object({ canvasId: idSchema, serverId: idSchema, x: coordSchema, y: coordSchema }),
	async ({ serverId, x, y }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			orchestrationClient().moveServer({ serverId, position: { x, y } }, { metadata })
		);
		return { ok: true as const };
	}
);

export const deleteServerNode = command(
	v.object({
		canvasId: idSchema,
		serverId: idSchema,
		force: v.optional(v.boolean(), false)
	}),
	async ({ canvasId, serverId, force }) => {
		const metadata = sessionMetadata(requireSessionId());
		// `DeleteServer` refuses while any pod still points at one of its ips, so the
		// pods go first. The list is re-read here rather than taken from the client:
		// a pod added since the last refresh would otherwise block the delete.
		const detail = await callGrpc(() =>
			orchestrationClient().getCanvas({ canvasId }, { metadata })
		);
		const ownIps = new Set(
			detail.servers.find(server => server.id === serverId)?.ips.map(ip => ip.id) ?? []
		);
		for (const node of detail.nodes) {
			const pod = node.spec?.pod;
			if (!pod || !ownIps.has(pod.ipRecordId)) continue;
			const nodeId = node.id;
			await callGrpc(() =>
				force
					? orchestrationClient().forceDeleteNode({ nodeId }, { metadata })
					: orchestrationClient().retireNode({ nodeId }, { metadata })
			);
		}
		await callGrpc(() => orchestrationClient().deleteServer({ serverId }, { metadata }));
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const addServerIpAddress = command(
	v.object({
		canvasId: idSchema,
		serverId: idSchema,
		ip: ipSchema,
		country: v.optional(v.pipe(v.string(), v.trim()), '')
	}),
	async ({ canvasId, serverId, ip, country }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			orchestrationClient().addServerIp({ serverId, ip, country }, { metadata })
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const removeServerIpAddress = command(
	v.object({ canvasId: idSchema, ipRecordId: idSchema }),
	async ({ canvasId, ipRecordId }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() => orchestrationClient().removeServerIp({ ipRecordId }, { metadata }));
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

const standaloneKindSchema = v.picklist([
	'entry',
	'relay',
	'exit',
	'load_balance_distribute',
	'load_balance_aggregate'
] as const);

export const createStandaloneNode = command(
	v.object({
		canvasId: idSchema,
		kind: standaloneKindSchema,
		name: nameSchema,
		x: coordSchema,
		y: coordSchema,
		memberCount: v.optional(memberCountSchema, 2)
	}),
	async ({ canvasId, kind, name, x, y, memberCount }) => {
		const metadata = sessionMetadata(requireSessionId());
		// A half-drawn chain is deliberately storable (see `services/topology.rs`):
		// an exit with no destination yet is a warning in the problems panel, not a
		// write rejection, so the palette never invents a placeholder value.
		const spec =
			kind === 'entry'
				? { entry: { receiveProxyProtocol: ProxyProtocolVersion.UNSPECIFIED, tls: undefined } }
				: kind === 'relay'
					? {
							relay: {
								protocol: RelayProtocol.RELAY_TCP_RAW,
								overrideIpAddress: '',
								overridePort: 0
							}
						}
					: kind === 'exit'
						? { exit: { destination: '', passProxyProtocol: ProxyProtocolVersion.UNSPECIFIED } }
						: kind === 'load_balance_distribute'
							? { loadBalanceDistribute: { mode: LoadBalanceMode.ROUND_ROBIN } }
							: { loadBalanceAggregate: {} };
		const itemCount =
			kind === 'load_balance_distribute' || kind === 'load_balance_aggregate' ? memberCount : 0;

		await callGrpc(() =>
			orchestrationClient().createNode(
				{ canvasId, name, comment: '', spec, position: { x, y }, itemCount },
				{ metadata }
			)
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const createPodNode = command(
	v.object({
		canvasId: idSchema,
		name: nameSchema,
		ipRecordId: idSchema,
		port: portSchema,
		x: coordSchema,
		y: coordSchema
	}),
	async ({ canvasId, name, ipRecordId, port, x, y }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			orchestrationClient().createNode(
				{
					canvasId,
					name,
					comment: '',
					spec: { pod: { ipRecordId, port } },
					position: { x, y },
					itemCount: 0
				},
				{ metadata }
			)
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const updateNodeText = command(
	v.object({
		canvasId: idSchema,
		nodeId: idSchema,
		name: nameSchema,
		comment: commentSchema
	}),
	async ({ canvasId, nodeId, name, comment }) => {
		const metadata = sessionMetadata(requireSessionId());
		// An unset `position` means "do not move".
		await callGrpc(() =>
			orchestrationClient().updateNodeMeta(
				{ nodeId, name, comment, position: undefined },
				{ metadata }
			)
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

/**
 * `UpdateNodeMeta` replaces name and comment wholesale, so a move has to resend
 * them. Deliberately does not refresh: the dragged position already matches.
 */
export const moveNode = command(
	v.object({
		canvasId: idSchema,
		nodeId: idSchema,
		name: nameSchema,
		comment: commentSchema,
		x: coordSchema,
		y: coordSchema
	}),
	async ({ nodeId, name, comment, x, y }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			orchestrationClient().updateNodeMeta(
				{ nodeId, name, comment, position: { x, y } },
				{ metadata }
			)
		);
		return { ok: true as const };
	}
);

export const replaceEntrySpec = command(
	v.object({
		canvasId: idSchema,
		nodeId: idSchema,
		receiveProxyProtocol: proxySchema,
		tls: tlsSchema
	}),
	async ({ canvasId, nodeId, receiveProxyProtocol, tls }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			orchestrationClient().replaceNodeSpec(
				{
					nodeId,
					// TLS is not editable here; it is round-tripped so the replace keeps it.
					spec: {
						entry: { receiveProxyProtocol: fromProxy(receiveProxyProtocol), tls: tls ?? undefined }
					},
					itemCount: 0
				},
				{ metadata }
			)
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const replaceRelaySpec = command(
	v.object({
		canvasId: idSchema,
		nodeId: idSchema,
		protocol: relayProtocolSchema,
		overrideIpAddress: v.optional(v.pipe(v.string(), v.trim()), ''),
		overridePort: v.pipe(v.number(), v.integer(), v.minValue(0), v.maxValue(65535))
	}),
	async ({ canvasId, nodeId, protocol, overrideIpAddress, overridePort }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			orchestrationClient().replaceNodeSpec(
				{
					nodeId,
					spec: {
						relay: { protocol: fromRelayProtocol(protocol), overrideIpAddress, overridePort }
					},
					itemCount: 0
				},
				{ metadata }
			)
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const replaceExitSpec = command(
	v.object({
		canvasId: idSchema,
		nodeId: idSchema,
		destination: v.optional(v.pipe(v.string(), v.trim()), ''),
		passProxyProtocol: proxySchema
	}),
	async ({ canvasId, nodeId, destination, passProxyProtocol }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			orchestrationClient().replaceNodeSpec(
				{
					nodeId,
					spec: { exit: { destination, passProxyProtocol: fromProxy(passProxyProtocol) } },
					itemCount: 0
				},
				{ metadata }
			)
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const replaceLoadBalanceSpec = command(
	v.object({
		canvasId: idSchema,
		nodeId: idSchema,
		mode: v.picklist(['distribute', 'aggregate'] as const),
		balanceMode: balanceModeSchema,
		memberCount: memberCountSchema
	}),
	async ({ canvasId, nodeId, mode, balanceMode, memberCount }) => {
		const metadata = sessionMetadata(requireSessionId());
		// The spec kind cannot change, so `mode` only picks which config to resend.
		await callGrpc(() =>
			orchestrationClient().replaceNodeSpec(
				{
					nodeId,
					spec:
						mode === 'distribute'
							? { loadBalanceDistribute: { mode: fromBalanceMode(balanceMode) } }
							: { loadBalanceAggregate: {} },
					itemCount: memberCount
				},
				{ metadata }
			)
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const replacePodSpec = command(
	v.object({
		canvasId: idSchema,
		nodeId: idSchema,
		ipRecordId: idSchema,
		port: portSchema
	}),
	async ({ canvasId, nodeId, ipRecordId, port }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			orchestrationClient().replaceNodeSpec(
				{ nodeId, spec: { pod: { ipRecordId, port } }, itemCount: 0 },
				{ metadata }
			)
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const deleteNode = command(
	v.object({ canvasId: idSchema, nodeId: idSchema, force: v.optional(v.boolean(), false) }),
	async ({ canvasId, nodeId, force }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			force
				? orchestrationClient().forceDeleteNode({ nodeId }, { metadata })
				: orchestrationClient().retireNode({ nodeId }, { metadata })
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const connectNodePorts = command(
	v.object({ canvasId: idSchema, outputPortId: idSchema, inputPortId: idSchema }),
	async ({ canvasId, outputPortId, inputPortId }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			orchestrationClient().connectPorts({ outputPortId, inputPortId }, { metadata })
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);

export const disconnectEdge = command(
	v.object({ canvasId: idSchema, edgeId: idSchema, force: v.optional(v.boolean(), false) }),
	async ({ canvasId, edgeId, force }) => {
		const metadata = sessionMetadata(requireSessionId());
		await callGrpc(() =>
			force
				? orchestrationClient().forceDisconnect({ edgeId }, { metadata })
				: orchestrationClient().disconnect({ edgeId }, { metadata })
		);
		await getCanvasGraph({ canvasId }).refresh();
		return { ok: true as const };
	}
);
