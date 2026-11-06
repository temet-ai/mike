import { getSessionMessages, sendSessionMessage } from '$lib/api/mailbox';
import type {
	MailboxEnvelope,
	SendMailboxMessageBody,
	SendMailboxMessageResponse
} from '$lib/api/mailbox';
import { ApiError } from '$lib/api';
import { onApiCacheUpdate } from '$lib/stores/cache/apiCache.svelte';
import type { ProxyEvent } from '$lib/types';

export interface SessionMailboxStore {
	readonly currentId: string | null;
	readonly envelope: MailboxEnvelope | null;
	readonly loading: boolean;
	readonly loaded: boolean;
	readonly error: string | null;
	load(id: string): Promise<void>;
	reload(): Promise<void>;
	send(toSessionId: string, body: SendMailboxMessageBody): Promise<SendMailboxMessageResponse>;
	handleEvent(event: ProxyEvent): void;
}

interface MutableState {
	currentId: string | null;
	envelope: MailboxEnvelope | null;
	loading: boolean;
	loaded: boolean;
	error: string | null;
}

export function createSessionMailboxStore(): SessionMailboxStore {
	const state = $state<MutableState>({
		currentId: null,
		envelope: null,
		loading: false,
		loaded: false,
		error: null
	});

	let inFlight: Promise<void> | null = null;
	let inFlightId: string | null = null;
	let unsubscribeRevalidation: (() => void) | null = null;

	function fetchOnce(id: string): Promise<void> {
		if (inFlight && inFlightId === id) return inFlight;
		const promise = (async (): Promise<void> => {
			state.loading = true;
			state.error = null;
			try {
				const env = await getSessionMessages(id);
				if (state.currentId !== id) return;
				state.envelope = env;
				state.loaded = true;
			} catch (err) {
				if (state.currentId !== id) return;
				state.error = err instanceof ApiError ? err.message : String(err);
				state.envelope = null;
				state.loaded = true;
			} finally {
				if (state.currentId === id) state.loading = false;
				if (inFlightId === id) {
					inFlight = null;
					inFlightId = null;
				}
			}
		})();
		inFlight = promise;
		inFlightId = id;
		return promise;
	}

	function load(id: string): Promise<void> {
		if (state.currentId === id) {
			if (state.loaded) return Promise.resolve();
			return fetchOnce(id);
		}
		state.currentId = id;
		state.envelope = null;
		state.loaded = false;
		state.error = null;
		unsubscribeRevalidation?.();
		unsubscribeRevalidation = onApiCacheUpdate(
			`/api/sessions/${id}/messages`,
			() => {
				void reload();
			},
			{ prefix: false }
		);
		return fetchOnce(id);
	}

	function reload(): Promise<void> {
		const id = state.currentId;
		if (!id) return Promise.resolve();
		return fetchOnce(id);
	}

	async function send(
		toSessionId: string,
		body: SendMailboxMessageBody
	): Promise<SendMailboxMessageResponse> {
		const res = await sendSessionMessage(toSessionId, body);
		await reload();
		return res;
	}

	function handleEvent(event: ProxyEvent): void {
		const id = state.currentId;
		if (!id) return;
		if (event.type === 'Lagged') {
			void reload();
			return;
		}
		if (event.type !== 'RowChanged') return;
		if (event.table === 'mailbox_messages' && event.session_id === id) {
			void reload();
			return;
		}
		if (event.table === 'mailbox_threads') {
			void reload();
		}
	}

	return {
		get currentId() {
			return state.currentId;
		},
		get envelope() {
			return state.envelope;
		},
		get loading() {
			return state.loading;
		},
		get loaded() {
			return state.loaded;
		},
		get error() {
			return state.error;
		},
		load,
		reload,
		send,
		handleEvent
	};
}
