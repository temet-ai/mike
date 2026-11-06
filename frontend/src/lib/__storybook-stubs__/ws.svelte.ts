import type { ConnectionState, ProxyEvent } from '../types';

export const MAX_EVENTS = 1000;

export class ProxyEventStream {
	totalEvents: number = 0;
	#ring: Array<ProxyEvent | undefined> = new Array(MAX_EVENTS).fill(undefined);
	state: ConnectionState = 'disconnected';
	laggedCount: number = 0;
	get connected(): boolean {
		return this.state === 'connected';
	}
	get latestEvent(): ProxyEvent | null {
		return this.totalEvents === 0
			? null
			: (this.#ring[(this.totalEvents - 1) % MAX_EVENTS] ?? null);
	}
	at(i: number): ProxyEvent | undefined {
		if (i < 0 || i >= this.totalEvents) return undefined;
		if (i < this.totalEvents - MAX_EVENTS) return undefined;
		return this.#ring[i % MAX_EVENTS];
	}
	connect(): void {}
	disconnect(): void {}
	clear(): void {
		this.totalEvents = 0;
		this.#ring.fill(undefined);
	}
	__pushForTests(event: ProxyEvent): void {
		this.#ring[this.totalEvents % MAX_EVENTS] = event;
		this.totalEvents++;
		if (event.type === 'Lagged') {
			this.laggedCount += event.skipped;
		}
	}
}

export const proxyEvents = new ProxyEventStream();
