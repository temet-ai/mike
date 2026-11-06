export type Point = readonly [x: number, y: number];

export type NodeKind =
	'session' | 'peer' | 'agent' | 'group' | 'tool' | 'service' | 'outside' | 'terminal';

export type IdentityFamily = 'session' | 'agent' | 'tool' | 'mike' | 'outside' | 'terminal';

export const IDENTITY_OF: Readonly<Record<NodeKind, IdentityFamily>> = {
	session: 'session',
	peer: 'session',
	agent: 'agent',
	group: 'agent',
	tool: 'tool',
	service: 'mike',
	outside: 'outside',
	terminal: 'terminal'
};

export type NodeState =
	'quiet' | 'busy' | 'recent' | 'error' | 'selected' | 'pending' | 'hot' | 'response' | 'plain';

export type PadSide =
	| 'right'
	| 'righthi'
	| 'rightlo'
	| 'left'
	| 'lefthi'
	| 'leftlo'
	| 'top'
	| 'topl'
	| 'topc'
	| 'bottom'
	| 'bottomc'
	| 'bottomr';

export interface ChipRect {
	readonly key: string;
	readonly kind: NodeKind;
	readonly x: number;
	readonly y: number;
	readonly w: number;
	readonly h: number;
}

export type TraceKind =
	| 'silk'
	| 'capture'
	| 'upstream'
	| 'response'
	| 'toolcall'
	| 'return'
	| 'internet'
	| 'mikeout'
	| 'audit'
	| 'dispatch'
	| 'message';

export type TraceLayer = 'top' | 'bottom';

export interface TraceKindSpec {
	readonly layer: TraceLayer;
	readonly pattern: string;
	readonly colorToken: string;
	readonly packets: boolean;
}

export const TRACE_KINDS: Readonly<Record<TraceKind, TraceKindSpec>> = {
	silk: { layer: 'top', pattern: '', colorToken: '--border-strong', packets: false },
	capture: { layer: 'top', pattern: '', colorToken: '--ident-agent-fg', packets: true },
	upstream: { layer: 'top', pattern: '', colorToken: '--signal-active', packets: true },
	response: { layer: 'top', pattern: '', colorToken: '--vendor-anthropic', packets: true },
	toolcall: { layer: 'top', pattern: '8 3', colorToken: '--ident-agent-fg', packets: true },
	return: { layer: 'top', pattern: '8 3', colorToken: '--ident-tool-fg', packets: true },
	internet: { layer: 'top', pattern: '2 3', colorToken: '--ident-tool-fg', packets: true },
	mikeout: { layer: 'top', pattern: '', colorToken: '--ident-mike-fg', packets: true },
	audit: { layer: 'bottom', pattern: '1 4', colorToken: '--ident-mike-fg', packets: true },
	dispatch: { layer: 'bottom', pattern: '3 5', colorToken: '--ident-session-fg', packets: false },
	message: { layer: 'bottom', pattern: '', colorToken: '--ident-terminal-fg', packets: true }
};

export const CHAMFER = 6;
export const PITCH = 3;
export const TRACE_W = 1.5;
export const SILK_W = 1;
export const PAD = 6;
export const VIA_R = 3;
export const HOP_R = 4;

export interface Route {
	readonly key: string;
	readonly points: readonly Point[];
}

export interface Trace {
	readonly key: string;
	readonly routeKey: string;
	readonly kind: TraceKind;
	readonly points: readonly Point[];
	readonly colorToken?: string;
	readonly hops?: readonly Point[];
	readonly reverse?: boolean;
	readonly packets?: boolean;
	readonly opacity?: number;
	readonly width?: number;
}

export interface PadMark {
	readonly x: number;
	readonly y: number;
	readonly layer: TraceLayer;
	readonly colorToken: string;
	readonly n?: number;
	readonly vertical?: boolean;
}
