import { describe, expect, it } from 'vitest';
import { groupByProjectAndConversation, projectLabelOf } from './groupByConversation';

const item = (projectLabel: string, sessionHref: string, id: number) => ({
	id,
	projectLabel,
	projectPath: projectLabel === 'No project' ? null : `/work/${projectLabel}`,
	sessionHref,
	sessionTitle: `title ${sessionHref}`
});

describe('projectLabelOf', () => {
	it('takes the last folder of the path', () => {
		expect(projectLabelOf('/mnt/data/projects/work/ankh-morpork/night-watch/')).toBe(
			'night-watch'
		);
	});

	it('names a missing project', () => {
		expect(projectLabelOf(null)).toBe('No project');
		expect(projectLabelOf('(no project)')).toBe('No project');
		expect(projectLabelOf(undefined)).toBe('No project');
	});
});

describe('groupByProjectAndConversation', () => {
	it('groups by project then conversation, projects alphabetical with No project last', () => {
		const groups = groupByProjectAndConversation([
			item('No project', '/s/x', 1),
			item('zeta', '/s/a', 2),
			item('alpha', '/s/b', 3),
			item('zeta', '/s/c', 4),
			item('zeta', '/s/a', 5)
		]);

		expect(groups.map((g) => g.projectLabel)).toEqual(['alpha', 'zeta', 'No project']);
		const zeta = groups[1]!;
		expect(zeta.count).toBe(3);
		expect(zeta.projectPath).toBe('/work/zeta');
		expect(zeta.conversations.map((c) => c.href)).toEqual(['/s/a', '/s/c']);
		expect(zeta.conversations[0]!.items.map((i) => i.id)).toEqual([2, 5]);
		expect(zeta.conversations[0]!.title).toBe('title /s/a');
	});

	it('returns nothing for no items', () => {
		expect(groupByProjectAndConversation([])).toEqual([]);
	});
});
