import { redirect } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ url }) => {
	const params = new URLSearchParams(url.search);
	params.set('for', 'sessions');
	redirect(308, `/sessions/labelling?${params.toString()}`);
};
