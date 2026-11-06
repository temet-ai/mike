export const base = '';
export const assets = '';
export const resolve = (id: string, params?: Record<string, string>): string => {
	if (params === undefined) return id;
	return id.replace(/\[(\.\.\.)?([^\]]+)\]/g, (_m, _spread, name: string) => params[name] ?? '');
};
export const resolveRoute = resolve;
