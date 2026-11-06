import { describe, expect, it } from 'vitest';
import { fixSegments } from './fixLinks';

describe('fixSegments', () => {
	it('turns an external URL into a link and keeps the sentence full stop as text', () => {
		expect(fixSegments('Add credit at https://openrouter.ai/settings/credits. Workers resume.')).toEqual([
			{ kind: 'text', text: 'Add credit at ' },
			{
				kind: 'link',
				text: 'https://openrouter.ai/settings/credits',
				href: 'https://openrouter.ai/settings/credits',
				external: true
			},
			{ kind: 'text', text: '. Workers resume.' }
		]);
	});

	it('turns an in-app path into an internal link', () => {
		expect(fixSegments('Enable embedding.enabled at /settings/embedding')).toEqual([
			{ kind: 'text', text: 'Enable embedding.enabled at ' },
			{ kind: 'link', text: '/settings/embedding', href: '/settings/embedding', external: false }
		]);
	});

	it('leaves text without links alone', () => {
		expect(fixSegments('Check that Postgres is reachable')).toEqual([
			{ kind: 'text', text: 'Check that Postgres is reachable' }
		]);
	});

	it('does not treat a slash inside a word as a path', () => {
		expect(fixSegments('read/write access')).toEqual([{ kind: 'text', text: 'read/write access' }]);
	});
});
