import type { ConnectionState, ProxyEvent, ResyncReason, RowChangedTable } from '$lib/types';
import type { SessionFlowComponent } from '$lib/api/generated/SessionFlowComponent';
import type { SessionFlowStore } from './sessionFlowStore.svelte';

const SESSION_SCOPED_COMPONENTS: Partial<Record<RowChangedTable, readonly SessionFlowComponent[]>> =
	{
		interventions: ['interventions'],
		mailbox_messages: ['messages'],
		lesson_injections: ['lesson_injections'],
		steering_evaluations: ['verifier_evaluations'],
		consolidation_actions: ['consolidation'],
		session_judges_verdicts: ['judgments'],
		llm_calls: ['llm_calls'],
		session_embeddings: ['embeddings']
	};

const UNSCOPED_COMPONENTS: Partial<Record<RowChangedTable, readonly SessionFlowComponent[]>> = {
	mailbox_threads: ['messages'],
	consolidation_runs: ['consolidation']
};

function humanizeResyncReason(reason: ResyncReason): string {
	switch (reason) {
		case 'listener_establishment':
			return 'live listener just started';
		case 'recovery':
			return 'live listener recovered from a dropped connection';
		case 'unusable_notification':
			return 'a live notification could not be parsed';
	}
}

export interface SessionFlowLiveAdapter {
	handleEvent(event: ProxyEvent): void;
	observeConnectionState(state: ConnectionState): void;
	dispose(): void;
}

export function createSessionFlowLiveAdapter(options: {
	flowStore: SessionFlowStore;
	resolveRequestSessionId: (requestId: number) => string | null;
}): SessionFlowLiveAdapter {
	const { flowStore, resolveRequestSessionId } = options;
	let disposed = false;
	let lastConnectionState: ConnectionState | null = null;

	function invalidateAll(reason: string): void {
		flowStore.invalidate(reason);
	}

	function invalidateScoped(reason: string, components: readonly SessionFlowComponent[]): void {
		flowStore.invalidate(reason, components);
	}

	function handleRowChanged(
		event: Extract<ProxyEvent, { type: 'RowChanged' }>,
		sessionId: string
	): void {
		if (event.table === 'sessions') {
			if (event.id === sessionId) invalidateAll('Session record changed');
			return;
		}
		if (event.table === 'requests') {
			if (event.session_id === sessionId) invalidateAll('Request data changed');
			return;
		}
		const unscoped = UNSCOPED_COMPONENTS[event.table];
		if (unscoped) {
			invalidateScoped(`${event.table} changed`, unscoped);
			return;
		}
		const scoped = SESSION_SCOPED_COMPONENTS[event.table];
		if (scoped && event.session_id === sessionId)
			invalidateScoped(`${event.table} changed`, scoped);
	}

	function handleEvent(event: ProxyEvent): void {
		if (disposed) return;
		const sessionId = flowStore.activeSessionId;
		if (sessionId === null) return;
		switch (event.type) {
			case 'RowChanged':
				handleRowChanged(event, sessionId);
				return;
			case 'ResyncRequired':
				invalidateAll(`Listener resync: ${humanizeResyncReason(event.reason)}`);
				return;
			case 'Lagged':
				flowStore.clearLiveHistory('WebSocket ring buffer dropped frames');
				invalidateAll('WebSocket lagged; Flow data may be behind');
				return;
			case 'StreamingStarted':
				if (resolveRequestSessionId(event.id) !== sessionId) return;
				flowStore.observeLive({
					sessionId,
					requestId: String(event.id),
					family: 'StreamingStarted',
					receivedAtMs: Date.now(),
					statusCode: event.status,
					ttfbMs: event.ttfb_ms
				});
				return;
			case 'DiagnosticReady':
				if (resolveRequestSessionId(event.id) !== sessionId) return;
				flowStore.observeLive({
					sessionId,
					requestId: String(event.id),
					family: 'DiagnosticReady',
					receivedAtMs: Date.now(),
					count: event.count
				});
				return;
			default:
				return;
		}
	}

	function observeConnectionState(state: ConnectionState): void {
		if (disposed) return;
		const previous = lastConnectionState;
		lastConnectionState = state;
		if (state === 'connected' && (previous === 'disconnected' || previous === 'reconnecting')) {
			invalidateAll(`WebSocket reconnected after ${previous}; Flow data may be behind`);
		}
	}

	return {
		handleEvent,
		observeConnectionState,
		dispose() {
			disposed = true;
		}
	};
}
