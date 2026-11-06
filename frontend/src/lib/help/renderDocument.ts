import MarkdownIt from 'markdown-it';

export interface DocumentSection {
	slug: string;
	label: string;
}

/** Stable heading anchors let installed help retain section links across a refresh. */
export function renderDocument(content: string): { html: string; sections: DocumentSection[] } {
	const markdown = new MarkdownIt({ html: false, linkify: false });
	const sections: DocumentSection[] = [];
	const seen = new Map<string, number>();
	markdown.renderer.rules.heading_open = (tokens, index, options, _environment, renderer) => {
		const heading = tokens[index];
		if (!heading) throw new Error('Missing document heading token');
		const label = tokens[index + 1]?.content ?? '';
		const base =
			label
				.toLowerCase()
				.replace(/[^a-z0-9]+/g, '-')
				.replace(/^-|-$/g, '') || 'section';
		const occurrence = (seen.get(base) ?? 0) + 1;
		seen.set(base, occurrence);
		const slug = occurrence === 1 ? base : `${base}-${occurrence}`;
		heading.attrSet('id', slug);
		if (heading.tag === 'h2') sections.push({ slug, label });
		return renderer.renderToken(tokens, index, options);
	};
	return { html: markdown.render(content), sections };
}
