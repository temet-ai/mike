import type { SessionFlowActivity } from '$lib/api/generated/SessionFlowActivity';
import type { SessionFlowActor } from '$lib/api/generated/SessionFlowActor';
import type { SessionFlowComponent } from '$lib/api/generated/SessionFlowComponent';
import type { SessionFlowCoverage } from '$lib/api/generated/SessionFlowCoverage';
import type { SessionFlowOutcome } from '$lib/api/generated/SessionFlowOutcome';
import type { SessionFlowRelationship } from '$lib/api/generated/SessionFlowRelationship';
import type { SessionFlowSource } from '$lib/api/generated/SessionFlowSource';
import type { TraceKind } from '$lib/board/types';
import type { FlowIntervalIndex } from './intervalIndex';

export type FlowTimeIssue =
	| 'unknown_occurrence'
	| 'invalid_occurrence'
	| 'invalid_recorded'
	| 'invalid_end'
	| 'end_before_start';
export interface FlowActivity extends SessionFlowActivity {
	occurredAtMs: number | null;
	recordedAtMs: number | null;
	endedAtMs: number | null;
	atMs: number | null;
	timeIssues: readonly FlowTimeIssue[];
}
export interface FlowRelationship extends SessionFlowRelationship {
	occurredAtMs: number | null;
	recordedAtMs: number | null;
	atMs: number | null;
}
export interface FlowBounds {
	startMs: number;
	endMs: number;
}
export interface FlowDatasetInput {
	sessionId: string;
	actors?: readonly SessionFlowActor[];
	activities?: readonly SessionFlowActivity[];
	relationships?: readonly SessionFlowRelationship[];
	coverage?: readonly SessionFlowCoverage[];
}
export interface FlowDataset {
	sessionId: string;
	actors: ReadonlyMap<string, SessionFlowActor>;
	activities: ReadonlyMap<string, FlowActivity>;
	relationships: ReadonlyMap<string, FlowRelationship>;
	coverage: ReadonlyMap<SessionFlowComponent, SessionFlowCoverage>;
	observationFrontierMs: number | null;
	bounds: FlowBounds | null;
	index: FlowIntervalIndex;
}
export interface FlowScene {
	atMs: number;
	actors: readonly SessionFlowActor[];
	activities: readonly FlowActivity[];
	relationships: readonly FlowRelationship[];
	openCaptureIds: ReadonlySet<string>;
	pointIds: ReadonlySet<string>;
	outcomes: ReadonlyMap<string, SessionFlowOutcome>;
	unknownActivityIds: readonly string[];
	counts: {
		actors: number;
		activities: number;
		openCaptures: number;
		points: number;
		outcomes: number;
	};
	coverage: FlowDataset['coverage'];
}
export interface FlowLinkEndpoint {
	readonly key: string;
	readonly label: string;
}
export interface FlowLinkSelection {
	kind: 'link';
	routeKey: string;
	traceKind: TraceKind;
	endpoints: readonly FlowLinkEndpoint[];
	activityCount: number | null;
}
export type FlowSelection =
	| { kind: 'actor'; id: string }
	| { kind: 'activity'; id: string }
	| { kind: 'source'; source: SessionFlowSource }
	| FlowLinkSelection
	| null;
export interface FlowViewState {
	selection: FlowSelection;
	highlight: 'none' | 'predecessors' | 'dependents' | 'exchanges';
	brush: FlowBounds | null;
}
export type FlowPlaybackMode = 'paused' | 'playing' | 'live';
export interface FlowScheduler {
	now(): number;
	requestFrame(callback: () => void): number;
	cancelFrame(handle: number): void;
}
export type FlowSeekResult =
	| { status: 'positioned'; atMs: number }
	| { status: 'unavailable'; requestedAtMs: number; bounds: FlowBounds | null };
export interface FlowSeekOptions {
	keepPlaying?: boolean;
}
export type FlowSeekHandler = (atMs: number, options?: FlowSeekOptions) => void;
export interface FlowPlayback {
	readonly atMs: number | null;
	readonly mode: FlowPlaybackMode;
	readonly speed: number;
	readonly seekResult: FlowSeekResult | null;
	setDataset(dataset: FlowDataset): void;
	play(): void;
	pause(): void;
	seek(atMs: number, options?: FlowSeekOptions): FlowSeekResult;
	restart(): void;
	end(): void;
	returnToLive(): void;
	setSpeed(speed: number): void;
	resume(): void;
	dispose(): void;
}
