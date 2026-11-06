import { untrack } from 'svelte';
import { isSidecarOrigin } from '$lib/turn-origins';
import { isRequestLive } from '$lib/status';
import type {
	BodyPayload,
	DeltaRowDto,
	RequestDetail,
	TransformationManifestPayload
} from '$lib/types';
import { ApiError, getRequestDeltas, getRequestTransformationManifest } from '$lib/api';
import {
	describeBodyError,
	loadBody,
	peekBody,
	type BodyKind
} from '$lib/stores/cache/unifiedCache.svelte';
import { walkMessageDelta, type MessageDelta } from '$lib/components/message-view/messageWalker';
import {
	classifyShapeBeacon,
	type SpaDeltaFallbackReason as ExtractedSpaDeltaFallbackReason
} from './deltaFallback';

export interface LazyBody {
	readonly text: BodyPayload | null;
	readonly loading: boolean;
	readonly error: string | null;
	retry(): void;
}

export function useLazyBody(detailFn: () => RequestDetail | null, kind: BodyKind): LazyBody {
	let text = $state<BodyPayload | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let loadedId: number | null = null;

	function readSize(detail: RequestDetail): number {
		if (kind === 'response') return detail.response_body_size;
		return detail.request_body_size;
	}

	function isSkippedForStreaming(detail: RequestDetail): boolean {
		return kind === 'response' && detail.is_streaming && isRequestLive(detail.status);
	}

	function fetchBody(detail: RequestDetail): void {
		const id = detail.id;
		const cached = peekBody(id, kind);
		if (cached) {
			text = cached;
			error = null;
			loadedId = id;
			return;
		}
		loading = true;
		error = null;
		loadBody(id, kind)
			.then((payload) => {
				const current = detailFn();
				if (!current || current.id !== id) return;
				text = payload;
				loadedId = id;
			})
			.catch((err) => {
				const current = detailFn();
				if (!current || current.id !== id) return;
				error = describeBodyError(err);
			})
			.finally(() => {
				const current = detailFn();
				if (!current || current.id !== id) return;
				loading = false;
			});
	}

	$effect(() => {
		const detail = detailFn();
		const id = detail?.id ?? null;
		const size = detail ? readSize(detail) : null;
		const streaming = detail ? isSkippedForStreaming(detail) : false;

		untrack(() => {
			if (!detail) {
				text = null;
				error = null;
				loading = false;
				loadedId = null;
				return;
			}
			if (streaming) {
				text = null;
				error = null;
				loading = false;
				loadedId = id;
				return;
			}
			if (size === 0) {
				text = null;
				error = null;
				loading = false;
				loadedId = id;
				return;
			}
			if (loadedId === id && text !== null) return;
			fetchBody(detail);
		});
	});

	function retry(): void {
		const detail = detailFn();
		if (!detail) return;
		untrack(() => {
			fetchBody(detail);
		});
	}

	return {
		get text(): BodyPayload | null {
			return text;
		},
		get loading(): boolean {
			return loading;
		},
		get error(): string | null {
			return error;
		},
		retry
	};
}

export type SpaDeltaFallbackReason = ExtractedSpaDeltaFallbackReason;

export interface LazyDelta {
	readonly delta: MessageDelta | null;
	readonly deltaBody: BodyPayload | null;
	readonly removed: DeltaRowDto[];
	readonly full: BodyPayload | null;
	readonly fullBytes: number | null;
	readonly mode: 'delta' | 'full';
	readonly loading: boolean;
	readonly error: string | null;
	retry(): void;
	viewFull(): void;
	viewDelta(): void;
}

function recordFallback(reason: SpaDeltaFallbackReason): void {
	void fetch('/api/diagnostics/spa_delta_fallback', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ reason })
	}).catch(() => {});
}

function parseJsonSafe(raw: string | null): { value: unknown; ok: boolean } {
	if (raw === null) return { value: null, ok: true };
	try {
		return { value: JSON.parse(raw), ok: true };
	} catch {
		return { value: null, ok: false };
	}
}

function syntheticDelta(commonLen: number, currLen: number): MessageDelta {
	return {
		messages_added: [],
		commonLen,
		currLen,
		systemChanged: false,
		toolsChanged: false,
		modelChanged: false,
		paramsChanged: false,
		shape: 'tail-append'
	};
}

export function assembleDeltaBody(deltas: DeltaRowDto[]): {
	body: BodyPayload;
	commonLen: number;
	currLen: number;
} | null {
	const added = deltas.filter((d) => d.change_kind === 'added').sort((a, b) => a.ord - b.ord);
	if (added.length === 0) return null;
	const messages = added.map((d) => d.body);
	const text = JSON.stringify({ messages }, null, 2);
	const commonLen = added[0]!.ord;
	const currLen = added[added.length - 1]!.ord + 1;
	return {
		body: {
			text,
			byte_size: new TextEncoder().encode(text).length,
			has_body: true
		},
		commonLen,
		currLen
	};
}

export function useLazyDelta(
	detailFn: () => RequestDetail | null,
	predecessorIdFn: () => number | null
): LazyDelta {
	let delta = $state<MessageDelta | null>(null);
	let deltaBody = $state<BodyPayload | null>(null);
	let removed = $state<DeltaRowDto[]>([]);
	let full = $state<BodyPayload | null>(null);
	let fullBytes = $state<number | null>(null);
	let mode = $state<'delta' | 'full'>('delta');
	let loading = $state(false);
	let error = $state<string | null>(null);

	let loadedFor: number | null = null;
	let beaconedFor: { detailId: number; predId: number | null } | null = null;

	function beaconOnce(
		reason: SpaDeltaFallbackReason,
		target: { detailId: number; predId: number | null }
	): void {
		if (
			beaconedFor !== null &&
			beaconedFor.detailId === target.detailId &&
			beaconedFor.predId === target.predId
		) {
			return;
		}
		beaconedFor = target;
		recordFallback(reason);
	}

	function classifyAndWalkFallback(detailId: number, predId: number | null): void {
		const currPayload = peekBody(detailId, 'request');
		const predPayload = predId !== null ? peekBody(predId, 'request') : null;
		if (!currPayload) return;

		full = currPayload;

		const currParsed = parseJsonSafe(currPayload.text);
		if (!currParsed.ok) {
			beaconOnce('parse-error', { detailId, predId });
			delta = {
				messages_added: [],
				commonLen: 0,
				currLen: 0,
				systemChanged: false,
				toolsChanged: false,
				modelChanged: false,
				paramsChanged: false,
				shape: 'lineage-broken'
			};
			deltaBody = null;
			return;
		}

		if (predId === null) {
			const live = detailFn();
			const origin =
				(live as (RequestDetail & { turn_origin?: string }) | null)?.turn_origin ?? 'primary';
			const isSidecar = isSidecarOrigin(origin);
			delta = walkMessageDelta(null, currParsed.value);
			deltaBody = null;
			const messageCount = (live as RequestDetail | null)?.message_count ?? 0;
			if (!isSidecar && messageCount > 1) {
				beaconOnce('predecessor-not-loaded', { detailId, predId });
			}
			return;
		}

		if (!predPayload) return;

		const predParsed = parseJsonSafe(predPayload.text);
		if (!predParsed.ok) {
			beaconOnce('parse-error', { detailId, predId });
			delta = walkMessageDelta(null, currParsed.value);
			deltaBody = null;
			return;
		}

		const out = walkMessageDelta(predParsed.value, currParsed.value);
		delta = out;
		if (out.shape === 'tail-append' && out.commonLen < out.currLen) {
			const currRec = currParsed.value as Record<string, unknown>;
			const allMessages = Array.isArray(currRec.messages) ? currRec.messages : [];
			const tail = allMessages.slice(out.commonLen);
			const tailBodyText = JSON.stringify({ messages: tail }, null, 2);
			deltaBody = {
				text: tailBodyText,
				byte_size: new TextEncoder().encode(tailBodyText).length,
				has_body: true
			};
		} else {
			deltaBody = null;
		}
		const reason = classifyShapeBeacon({
			shape: out.shape,
			currLen: out.currLen,
			prev: predParsed.value,
			curr: currParsed.value
		});
		if (reason !== null) beaconOnce(reason, { detailId, predId });
	}

	function runFallback(detailId: number, predId: number | null): void {
		const currPromise = loadBody(detailId, 'request');
		const predPromise =
			predId !== null ? loadBody(predId, 'request') : Promise.resolve<BodyPayload | null>(null);

		Promise.all([
			currPromise,
			predPromise.catch((err) => {
				console.warn('[useLazyDelta] predecessor fetch failed', err);
				return null;
			})
		])
			.then(() => {
				const live = detailFn();
				if (!live || live.id !== detailId) return;
				classifyAndWalkFallback(detailId, predId);
			})
			.catch((err: unknown) => {
				const live = detailFn();
				if (!live || live.id !== detailId) return;
				error = describeBodyError(err);
			})
			.finally(() => {
				const live = detailFn();
				if (!live || live.id !== detailId) return;
				loading = false;
			});
	}

	function runDeltaFetch(detailId: number, predId: number | null): void {
		loading = true;
		error = null;
		const sessionId = detailFn()?.session_id;
		if (!sessionId) {
			runFallback(detailId, predId);
			return;
		}
		getRequestDeltas(sessionId, detailId)
			.then((payload) => {
				const live = detailFn();
				if (!live || live.id !== detailId) return;

				if (payload.deltas.length === 0) {
					runFallback(detailId, predId);
					return;
				}

				const assembled = assembleDeltaBody(payload.deltas);
				removed = payload.deltas
					.filter((d) => d.change_kind === 'removed')
					.sort((a, b) => a.ord - b.ord);

				if (assembled) {
					deltaBody = assembled.body;
					delta = syntheticDelta(assembled.commonLen, assembled.currLen);
				} else {
					deltaBody = null;
					delta = syntheticDelta(0, 0);
				}
				loading = false;
			})
			.catch((err: unknown) => {
				const live = detailFn();
				if (!live || live.id !== detailId) return;
				if (err instanceof ApiError) {
					runFallback(detailId, predId);
					return;
				}
				error = describeBodyError(err);
				loading = false;
			});
	}

	function ensureFull(detailId: number): void {
		if (full !== null) return;
		const cached = peekBody(detailId, 'request');
		if (cached) {
			full = cached;
			return;
		}
		loading = true;
		loadBody(detailId, 'request')
			.then((payload) => {
				const live = detailFn();
				if (!live || live.id !== detailId) return;
				full = payload;
			})
			.catch((err: unknown) => {
				const live = detailFn();
				if (!live || live.id !== detailId) return;
				error = describeBodyError(err);
			})
			.finally(() => {
				const live = detailFn();
				if (!live || live.id !== detailId) return;
				loading = false;
			});
	}

	$effect(() => {
		const detail = detailFn();
		const predId = predecessorIdFn();
		const detailId = detail?.id ?? null;

		untrack(() => {
			if (!detail || detailId === null) {
				delta = null;
				deltaBody = null;
				removed = [];
				full = null;
				fullBytes = null;
				error = null;
				loading = false;
				loadedFor = null;
				return;
			}
			fullBytes = detail.request_body_size;
			if (detail.request_body_size === 0) {
				delta = walkMessageDelta(null, null);
				deltaBody = null;
				removed = [];
				full = { text: null, byte_size: 0, has_body: false };
				error = null;
				loading = false;
				loadedFor = detailId;
				return;
			}
			if (loadedFor === detailId) {
				return;
			}
			loadedFor = detailId;
			delta = null;
			deltaBody = null;
			removed = [];
			full = null;
			runDeltaFetch(detailId, predId);
		});
	});

	function retry(): void {
		const detail = detailFn();
		const predId = predecessorIdFn();
		if (!detail) return;
		untrack(() => {
			loadedFor = detail.id;
			delta = null;
			deltaBody = null;
			removed = [];
			full = null;
			error = null;
			runDeltaFetch(detail.id, predId);
		});
	}

	function viewFull(): void {
		mode = 'full';
		const detail = detailFn();
		if (!detail) return;
		untrack(() => {
			ensureFull(detail.id);
		});
	}

	function viewDelta(): void {
		mode = 'delta';
	}

	return {
		get delta(): MessageDelta | null {
			return delta;
		},
		get deltaBody(): BodyPayload | null {
			return deltaBody;
		},
		get removed(): DeltaRowDto[] {
			return removed;
		},
		get full(): BodyPayload | null {
			return full;
		},
		get fullBytes(): number | null {
			return fullBytes;
		},
		get mode(): 'delta' | 'full' {
			return mode;
		},
		get loading(): boolean {
			return loading;
		},
		get error(): string | null {
			return error;
		},
		retry,
		viewFull,
		viewDelta
	};
}

export interface LazyManifest {
	readonly payload: TransformationManifestPayload | null;
	readonly loading: boolean;
	readonly error: string | null;
	retry(): void;
}

export function useLazyManifest(detailFn: () => RequestDetail | null): LazyManifest {
	let payload = $state<TransformationManifestPayload | null>(null);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let loadedId: number | null = null;

	function fetchManifest(detail: RequestDetail): void {
		const id = detail.id;
		loading = true;
		error = null;
		getRequestTransformationManifest(id)
			.then((result) => {
				const current = detailFn();
				if (!current || current.id !== id) return;
				payload = result;
				loadedId = id;
			})
			.catch((err: unknown) => {
				const current = detailFn();
				if (!current || current.id !== id) return;
				error = describeBodyError(err);
			})
			.finally(() => {
				const current = detailFn();
				if (!current || current.id !== id) return;
				loading = false;
			});
	}

	$effect(() => {
		const detail = detailFn();
		const id = detail?.id ?? null;
		untrack(() => {
			if (!detail || id === null) {
				payload = null;
				error = null;
				loading = false;
				loadedId = null;
				return;
			}
			if (loadedId === id && payload !== null) return;
			fetchManifest(detail);
		});
	});

	function retry(): void {
		const detail = detailFn();
		if (!detail) return;
		untrack(() => {
			fetchManifest(detail);
		});
	}

	return {
		get payload(): TransformationManifestPayload | null {
			return payload;
		},
		get loading(): boolean {
			return loading;
		},
		get error(): string | null {
			return error;
		},
		retry
	};
}
