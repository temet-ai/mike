export const page = {
	url: new URL('http://localhost/'),
	params: {} as Record<string, string>,
	route: { id: null as string | null },
	data: {} as Record<string, unknown>,
	form: null as unknown,
	state: {} as Record<string, unknown>,
	status: 200,
	error: null as Error | null
};
export const navigating = null as null | { from: unknown; to: unknown };
export const updated = { current: false, check: async () => false };
