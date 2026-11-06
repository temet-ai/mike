import { describe, expect, it } from 'vitest';
import { chooseFleetProject, LatestFleetRequest } from './projectSelection';
import type { FleetProjectSummary } from './types';

function project(project_path: string, last_seen = ''): FleetProjectSummary {
	return { project_path, first_seen: '', last_seen, pinned: false, custom_name: null };
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

	it('chooses the project with the most recent activity, not the first path in the alphabet', () => {
		const projects = [
			project('.claude', '2026-07-10T14:28:00.000000+00:00'),
			project('/work/busy', '2026-11-06T09:35:12.735528+00:00'),
			project('/work/idle', '2026-11-02T18:41:46.544020+00:00')
		];
		expect(chooseFleetProject('', projects)).toBe('/work/busy');
		expect(chooseFleetProject('', [...projects].reverse())).toBe('/work/busy');
	});

	it('prefers a project with a known last activity over one without', () => {
		const projects = [project('/work/alpha'), project('/work/zebra', '2026-11-06T09:35:12+00:00')];
		expect(chooseFleetProject('', projects)).toBe('/work/zebra');
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
