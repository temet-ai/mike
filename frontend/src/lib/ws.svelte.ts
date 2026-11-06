import type { ConnectionState, ProxyEvent } from './types';

export const MAX_EVENTS = 1000;

const MAX_RECONNECT_ATTEMPTS = 5;
const RECONNECT_BASE_MS = 500;
const RECONNECT_CAP_MS = 30_000;

function buildWsUrl(): string {
	if (typeof window === 'undefined') {
		return 'ws://127.0.0.1/api/ws';
	}
	const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
	return `${protocol}//${window.location.host}/api/ws`;
}

export class ProxyEventStream {
	totalEvents = $state<number>(0);

	#ring: Array<ProxyEvent | undefined> = new Array(MAX_EVENTS).fill(undefined);

	state = $state<ConnectionState>('disconnected');

	laggedCount = $state<number>(0);

	readonly connected = $derived(this.state === 'connected');

	#socket: WebSocket | null = null;
	#reconnectAttempts = 0;
	#reconnectTimer: ReturnType<typeof setTimeout> | null = null;
	#closedByUser = false;
	#url: string;

	constructor(url?: string) {
		this.#url = url ?? '';
	}

	connect(): void {
		if (typeof window === 'undefined') {
			return;
		}
		if (this.#socket && this.#socket.readyState !== WebSocket.CLOSED) {
			return;
		}
		this.#closedByUser = false;
		const url = this.#url || buildWsUrl();
		this.state = this.#reconnectAttempts === 0 ? 'connecting' : 'reconnecting';

		let socket: WebSocket;
		try {
			socket = new WebSocket(url);
		} catch (err) {
			console.error('[ws] failed to construct WebSocket', err);
			this.#scheduleReconnect();
			return;
		}
		this.#socket = socket;

		socket.addEventListener('open', () => {
			this.state = 'connected';
			this.#reconnectAttempts = 0;
		});

		socket.addEventListener('message', (ev: MessageEvent) => {
			if (typeof ev.data !== 'string') {
				return;
			}
			let parsed: ProxyEvent;
			try {
				parsed = JSON.parse(ev.data) as ProxyEvent;
			} catch (err) {
				console.warn('[ws] failed to parse frame', ev.data, err);
				return;
			}
			this.#push(parsed);
		});

		socket.addEventListener('close', () => {
			this.#socket = null;
			if (this.#closedByUser) {
				this.state = 'disconnected';
				return;
			}
			this.#scheduleReconnect();
		});

		socket.addEventListener('error', (ev) => {
			console.warn('[ws] socket error', ev);
		});
	}

	disconnect(): void {
		this.#closedByUser = true;
		if (this.#reconnectTimer !== null) {
			clearTimeout(this.#reconnectTimer);
			this.#reconnectTimer = null;
		}
		if (this.#socket) {
			this.#socket.close();
			this.#socket = null;
		}
		this.state = 'disconnected';
	}

	at(i: number): ProxyEvent | undefined {
		if (i < 0 || i >= this.totalEvents) return undefined;
		if (i < this.totalEvents - MAX_EVENTS) return undefined;
		return this.#ring[i % MAX_EVENTS];
	}

	clear(): void {
		this.totalEvents = 0;
		this.#ring.fill(undefined);
	}

	#push(event: ProxyEvent): void {
		if (event.type === 'Lagged') {
			this.laggedCount += event.skipped;
		}
		this.#ring[this.totalEvents % MAX_EVENTS] = event;
		this.totalEvents++;
	}

	__pushForTests(event: ProxyEvent): void {
		this.#push(event);
	}

	#scheduleReconnect(): void {
		if (this.#reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
			console.warn('[ws] max reconnect attempts reached — giving up');
			this.state = 'disconnected';
			return;
		}
		const delay = Math.min(RECONNECT_BASE_MS * 2 ** this.#reconnectAttempts, RECONNECT_CAP_MS);
		this.#reconnectAttempts += 1;
		this.state = 'reconnecting';
		if (this.#reconnectTimer !== null) {
			clearTimeout(this.#reconnectTimer);
		}
		this.#reconnectTimer = setTimeout(() => {
			this.#reconnectTimer = null;
			this.connect();
		}, delay);
	}
}

export const proxyEvents = new ProxyEventStream();
