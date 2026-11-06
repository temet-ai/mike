declare const typedBodyBrand: unique symbol;

/**
 * A response body that was written against a named contract type. The type
 * argument has no default that accepts anything: `typed({...})` without
 * `typed<SomeDto>({...})` is a compile error, and a field the DTO does not
 * have (a typo, a renamed or removed field) fails svelte-check.
 */
export type TypedBody = { readonly [typedBodyBrand]: true; readonly value: unknown };

export function typed<T = never>(data: NoInfer<T>): TypedBody {
	return { value: data } as unknown as TypedBody;
}

function bodyOf(body: TypedBody): unknown {
	return (body as unknown as { value: unknown }).value;
}

/** Answers every request with `data`; name the contract type: `stubFetchJson<AgentListItem[]>(...)`. */
export function stubFetchJson<T = never>(data: NoInfer<T>): void {
	globalThis.fetch = async () =>
		new Response(JSON.stringify(data), {
			status: 200,
			headers: { 'content-type': 'application/json' }
		});
}

export function stubFetchJsonWithPatch<T = never>(
	getData: NoInfer<T>,
	onPatch: (url: string, body: unknown) => TypedBody
): void {
	globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
		const url = typeof input === 'string' ? input : input.toString();
		if (init?.method === 'PATCH') {
			const body: unknown = init.body ? JSON.parse(init.body as string) : {};
			return new Response(JSON.stringify(bodyOf(onPatch(url, body))), {
				status: 200,
				headers: { 'content-type': 'application/json' }
			});
		}
		return new Response(JSON.stringify(getData), {
			status: 200,
			headers: { 'content-type': 'application/json' }
		});
	}) as typeof fetch;
}

export function stubFetchJsonRouter(
	handler: (method: string, url: string, body: unknown) => TypedBody
): void {
	globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
		const url = typeof input === 'string' ? input : input.toString();
		const method = init?.method ?? 'GET';
		const body: unknown = init?.body ? JSON.parse(init.body as string) : {};
		return new Response(JSON.stringify(bodyOf(handler(method, url, body))), {
			status: 200,
			headers: { 'content-type': 'application/json' }
		});
	}) as typeof fetch;
}

export function stubFetchError(message: string, status = 503): void {
	globalThis.fetch = async () =>
		new Response(JSON.stringify({ error: message }), {
			status,
			statusText: status === 503 ? 'Service Unavailable' : 'Error',
			headers: { 'content-type': 'application/json' }
		});
}

export function stubFetchErrorFor(path: string, message: string, status = 503): void {
	globalThis.fetch = async (input: RequestInfo | URL) => {
		const url = typeof input === 'string' ? input : input.toString();
		if (url.includes(path)) {
			return new Response(JSON.stringify({ error: message }), {
				status,
				statusText: status === 503 ? 'Service Unavailable' : 'Error',
				headers: { 'content-type': 'application/json' }
			});
		}
		return new Response('Not Found', { status: 404, statusText: 'Not Found' });
	};
}

export function stubFetch404(body: unknown = { error: 'not found' }): void {
	globalThis.fetch = async () =>
		new Response(JSON.stringify(body), {
			status: 404,
			statusText: 'Not Found'
		});
}

export function stubFetchPending(): void {
	globalThis.fetch = async () => new Promise<Response>(() => {});
}
