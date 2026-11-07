export const base = '';
export const assets = '';
export const resolve = (id: string, params?: Record<string, string>): string => {
	// Kit 3: a leading slash marks a route ID; anything else is a pathname that is
	// prefixed with the (empty) base and a slash.
	if (!id.startsWith('/')) return `/${id}`;
	if (params === undefined) return id;
	return id.replace(/\[(\.\.\.)?([^\]]+)\]/g, (_m, _spread, name: string) => params[name] ?? '');
};
export const resolveRoute = resolve;
