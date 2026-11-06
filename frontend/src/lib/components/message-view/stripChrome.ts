export function stripCacheControl(node: unknown): unknown {
	if (Array.isArray(node)) {
		return node.map(stripCacheControl);
	}
	if (node !== null && typeof node === 'object') {
		const out: Record<string, unknown> = {};
		for (const [k, v] of Object.entries(node as Record<string, unknown>)) {
			if (k === 'cache_control') continue;
			out[k] = stripCacheControl(v);
		}
		return out;
	}
	return node;
}

export function stripBillingHeader(system: unknown): unknown {
	if (!Array.isArray(system)) return system;
	return system.filter((block) => {
		if (block === null || typeof block !== 'object') return true;
		const text = (block as Record<string, unknown>).text;
		if (typeof text !== 'string') return true;
		return !text.startsWith('x-anthropic-billing-header:');
	});
}

export function deepEqual(a: unknown, b: unknown): boolean {
	if (a === b) return true;
	if (a === null || b === null) return false;
	if (typeof a !== typeof b) return false;
	if (typeof a !== 'object') return false;
	if (Array.isArray(a)) {
		if (!Array.isArray(b)) return false;
		if (a.length !== b.length) return false;
		for (let i = 0; i < a.length; i++) {
			if (!deepEqual(a[i], b[i])) return false;
		}
		return true;
	}
	if (Array.isArray(b)) return false;
	const aObj = a as Record<string, unknown>;
	const bObj = b as Record<string, unknown>;
	const aKeys = Object.keys(aObj);
	const bKeys = Object.keys(bObj);
	if (aKeys.length !== bKeys.length) return false;
	for (const k of aKeys) {
		if (!Object.prototype.hasOwnProperty.call(bObj, k)) return false;
		if (!deepEqual(aObj[k], bObj[k])) return false;
	}
	return true;
}
