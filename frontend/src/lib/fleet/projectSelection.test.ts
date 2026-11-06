import { describe, expect, it } from 'vitest';
import { chooseFleetProject, LatestFleetRequest } from './projectSelection';
import type { FleetProjectSummary } from './types';

function project(project_path: string): FleetProjectSummary {
	return { project_path, first_seen: '', last_seen: '', pinned: false, custom_name: null };
}

describe('fleet project selection', () => {
	it('keeps empty installations unselected', () => {
		expect(chooseFleetProject('', [])).toBe('');
	});

	it('chooses the only captured project without a maintainer fallback', () => {
		expect(chooseFleetProject('', [project('/home/user/new project')])).toBe(
			'/home/user/new project'
		);
	});

	it('chooses deterministically across API list order', () => {
		const projects = [project('/workspace/zebra'), project('/workspace/alpha')];
		expect(chooseFleetProject('', projects)).toBe('/workspace/alpha');
		expect(chooseFleetProject('', [...projects].reverse())).toBe('/workspace/alpha');
		expect(projects[0]!.project_path).toBe('/workspace/zebra');
	});

	it('honors explicit deep links and selections even outside the returned list', () => {
		expect(chooseFleetProject('/work/chosen', [project('/work/first')])).toBe('/work/chosen');
		expect(chooseFleetProject('/work/chosen', [])).toBe('/work/chosen');
	});

	it('does not turn an empty path into a selection', () => {
		expect(chooseFleetProject('', [project(''), project('/work/valid')])).toBe('/work/valid');
	});
});

describe('fleet response ownership', () => {
	it('rejects stale project and graph responses when a newer request has started', () => {
		const requests = new LatestFleetRequest();
		const first = requests.begin();
		expect(first()).toBe(true);
		const second = requests.begin();
		expect(first()).toBe(false);
		expect(second()).toBe(true);
	});

	it('rejects all outstanding results on selection change or unmount', () => {
		const requests = new LatestFleetRequest();
		const current = requests.begin();
		requests.invalidate();
		expect(current()).toBe(false);
		expect(requests.begin()()).toBe(true);
	});
});
