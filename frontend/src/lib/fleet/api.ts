import type {
	FleetErrorBody,
	FleetAcceptanceStatus,
	FleetDecisionModes,
	FleetGraphResponse,
	FleetRecord,
	FleetProjectSummary,
	FleetProjectWithCounts
} from './types';

export class FleetApiError extends Error {
	constructor(
		message: string,
		readonly status: number,
		readonly code: string
	) {
		super(message);
		this.name = 'FleetApiError';
	}
}

async function fleetGetJson<T>(path: string): Promise<T> {
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		let code = 'unknown';
		let message = `GET ${path} failed: ${res.status} ${res.statusText}`;
		try {
			const body = (await res.json()) as Partial<FleetErrorBody>;
			if (typeof body.error === 'string' && body.error.length > 0) code = body.error;
			if (typeof body.message === 'string' && body.message.length > 0) message = body.message;
		} catch {
			// Non-JSON errors retain the HTTP status and request path above.
		}
		throw new FleetApiError(message, res.status, code);
	}
	return (await res.json()) as T;
}

export function getFleetGraph(project: string, windowHours: number): Promise<FleetGraphResponse> {
	const params = new URLSearchParams({ project, window_hours: String(windowHours) });
	return fleetGetJson<FleetGraphResponse>(`/api/fleet/graph?${params.toString()}`);
}

export async function getFleetAcceptanceForSession(
	sessionId: string
): Promise<FleetAcceptanceStatus | null> {
	try {
		return await fleetGetJson<FleetAcceptanceStatus>(
			`/api/fleets/by-session/${encodeURIComponent(sessionId)}/status`
		);
	} catch (error) {
		if (error instanceof FleetApiError && error.status === 404) return null;
		throw error;
	}
}

export async function getFleetRecord(fleet: string): Promise<FleetRecord | null> {
	try {
		return await fleetGetJson<FleetRecord>(`/api/fleets/${encodeURIComponent(fleet)}/record`);
	} catch (error) {
		if (error instanceof FleetApiError && error.status === 404) return null;
		throw error;
	}
}

const DEFAULT_MODES: FleetDecisionModes = {
	stop_check: 'on',
	wake_check: 'on',
	triage: 'on',
	commit_message: 'on',
	effort: 'on'
};

export async function getFleetDecisionModes(fleet: string): Promise<FleetDecisionModes> {
	const view = await fleetGetJson<{ definition?: { decisions?: Partial<FleetDecisionModes> } }>(
		`/api/fleets/${encodeURIComponent(fleet)}`
	);
	return { ...DEFAULT_MODES, ...(view.definition?.decisions ?? {}) };
}

export async function getLaunchSeat(session: string): Promise<string | null> {
	try {
		const row = await fleetGetJson<{ seat: string | null }>(
			`/api/launches/${encodeURIComponent(session)}`
		);
		return row.seat;
	} catch (error) {
		if (error instanceof FleetApiError && error.status === 404) return null;
		throw error;
	}
}

export function getFleetProjects(): Promise<FleetProjectSummary[]> {
	return fleetGetJson<FleetProjectSummary[]>('/api/projects');
}

export function getFleetProjectsWithCounts(): Promise<FleetProjectWithCounts[]> {
	return fleetGetJson<FleetProjectWithCounts[]>('/api/projects?counts=true');
}

export function getFleetProject(projectPath: string): Promise<FleetProjectWithCounts> {
	return fleetGetJson<FleetProjectWithCounts>(
		`/api/projects/by-path/${encodeURIComponent(projectPath)}`
	);
}
