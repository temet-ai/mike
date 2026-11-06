import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ url }) => {
	const params = new URLSearchParams(url.search);
	if (params.get('judge') === 'session') {
		params.delete('judge');
		params.set('for', 'sessions');
	}
	const search = params.toString();
	redirect(308, `/sessions/labelling/accuracy${search ? `?${search}` : ''}`);
};
