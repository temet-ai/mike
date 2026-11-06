/** Public listener metadata only; never load credentials or the boot configuration. */
export interface ConnectionInfo {
	proxy_url: string | null;
	inspect_url: string | null;
	browser_url: string | null;
}

function endpoint(value: unknown): string | null {
	if (value === null) return null;
	if (typeof value !== 'string') throw new Error('Invalid connection information');
	const url = new URL(value);
	if (
		!['http:', 'https:'].includes(url.protocol) ||
		url.username ||
		url.password ||
		url.pathname !== '/' ||
		url.search ||
		url.hash
	)
		throw new Error('Invalid connection endpoint');
	return url.origin;
}

export async function getConnectionInfo(signal?: AbortSignal): Promise<ConnectionInfo> {
	const response = await fetch('/api/connection-info', {
		headers: { accept: 'application/json' },
		signal
	});
	if (!response.ok) throw new Error(`Connection information unavailable (${response.status})`);
	const body = (await response.json()) as Record<string, unknown>;
	return {
		proxy_url: endpoint(body.proxy_url),
		inspect_url: endpoint(body.inspect_url),
		browser_url: endpoint(body.browser_url)
	};
}

/** Quote even validated URLs: a copied shell command must never evaluate its contents. */
export function directClaudeCommand(proxyUrl: string): string {
	return `ANTHROPIC_BASE_URL='${proxyUrl.replaceAll("'", "'\\''")}' claude`;
}
