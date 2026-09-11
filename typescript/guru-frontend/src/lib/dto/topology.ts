import type { CanvasProblem } from '#lib/dto/canvas.js';

/**
 * The canvas editor's view of a topology. Protobuf never reaches the client:
 * numeric enums become string unions and `int64` positions become numbers.
 */

export type PortKindName = 'derive_listen' | 'derive_destination';
export type PortDirectionName = 'input' | 'output';
export type ProxyProtocolName = 'none' | 'v1' | 'v2';
export type RelayProtocolName = 'tcp_raw' | 'tcp_tls' | 'quic';
export type LoadBalanceModeName = 'round_robin' | 'random' | 'ip_hash' | 'fallback';
export type Ipv6ResolveName = 'required' | 'preferred' | 'tolerated' | 'forbidden';

export type CanvasPort = {
	id: string;
	kind: PortKindName;
	direction: PortDirectionName;
	/** Derived server-side (`port_layout`); the client never invents one. */
	key: string;
	position: number;
};

type NodeBase = {
	id: string;
	name: string;
	comment: string;
	x: number;
	y: number;
	ports: CanvasPort[];
};

/**
 * Round-tripped verbatim: the entry sheet never edits TLS, but `ReplaceNodeSpec`
 * replaces the whole spec, so dropping it here would clobber it.
 */
export type EntryTls = {
	sni: string;
	dnsProviderId: string;
	domainId: string;
	acmeDirectory: string;
} | null;

export type EntryNodeDto = NodeBase & {
	kind: 'entry';
	receiveProxyProtocol: ProxyProtocolName;
	tls: EntryTls;
};
export type RelayNodeDto = NodeBase & {
	kind: 'relay';
	protocol: RelayProtocolName;
	overrideIpAddress: string;
	overridePort: number;
};
export type ExitNodeDto = NodeBase & {
	kind: 'exit';
	destination: string;
	passProxyProtocol: ProxyProtocolName;
};
export type LoadBalanceNodeDto = NodeBase & {
	kind: 'load_balance';
	mode: 'distribute' | 'aggregate';
	/** Only meaningful for `distribute`; `aggregate` has no mode field in the proto. */
	balanceMode: LoadBalanceModeName;
	memberCount: number;
};
export type StandaloneNode = EntryNodeDto | RelayNodeDto | ExitNodeDto | LoadBalanceNodeDto;

export type PodDto = {
	id: string;
	name: string;
	comment: string;
	ipRecordId: string;
	port: number;
	ports: CanvasPort[];
};
export type ServerIpDto = { id: string; ip: string; country: string };
export type ServerDto = {
	id: string;
	name: string;
	icon: string;
	comment: string;
	x: number;
	y: number;
	ipv6Resolve: Ipv6ResolveName;
	logLevel: string;
	lastSeenAt: string;
	ips: ServerIpDto[];
	pods: PodDto[];
};

export type CanvasEdgeDto = { id: string; sourcePortId: string; targetPortId: string };
export type TopologyProblem = CanvasProblem & {
	nodeIds: string[];
	edgeIds: string[];
	portIds: string[];
};

export type CanvasGraph = {
	canvas: { id: string; name: string; description: string };
	servers: ServerDto[];
	nodes: StandaloneNode[];
	edges: CanvasEdgeDto[];
	problems: TopologyProblem[];
	/** Pods whose `ipRecordId` resolves to no server ip on this canvas. */
	orphanPods: PodDto[];
};
