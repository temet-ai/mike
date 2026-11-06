import { error } from '@sveltejs/kit';
import { DOCUMENT_CONTENT } from '$lib/help/documents';
import { PROJECT_DOCS } from '$lib/help/externalDocs';
import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => {
	const document = PROJECT_DOCS.find((entry) => entry.href === `/help/docs/${params.slug}`);
	const content = DOCUMENT_CONTENT[params.slug];
	if (!document || !content) error(404, 'This installed guide was not found.');
	return { document, content };
};
