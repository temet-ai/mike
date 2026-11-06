export type BackendId = 'herdr' | 'tmux' | 'terminal' | 'ghostty' | 'none';

export type ChannelCheck = 'found' | 'timeout' | 'db_unreachable' | 'no_session_id' | 'disabled';

export type LaunchResult = 'pending' | 'success' | 'unconfirmed' | 'failed';

export interface LaunchRow {
	id: number;
	session_uuid: string | null;
	backend: BackendId;
	folder: string;
	repo_root: string | null;
	worktree: boolean;
	workspace_ref: string | null;
	tab_ref: string | null;
	surface_ref: string | null;
	pid: number | null;
	run_id: string | null;
	seat: string | null;
	template_id: number | null;
	model: string | null;
	agent: string | null;
	claude_args: string[];
	prompt_present: boolean;
	brief_mode: 'inline' | 'file' | null;
	brief_path: string | null;
	brief_text: string | null;
	requested_placement: string | null;
	actual_placement: string | null;
	split_direction: string | null;
	anchor_ref: string | null;
	remote_control: boolean;
	proxy_env: boolean;
	channel_check: ChannelCheck | null;
	channel_checked_at: string | null;
	opened: boolean;
	delivered: boolean | null;
	confirmed: boolean;
	result: LaunchResult;
	reason: string | null;
	stderr_text: string | null;
	stderr_truncated: boolean;
	created_at: string;
	opened_at: string | null;
	closed_at: string | null;
	close_reason: string | null;
	close_process_exited: boolean | null;
}

export interface KillReport {
	session: string;
	surface: string | null;
	closed: boolean;
	process_exited: boolean | null;
}

export interface Check {
	name: string;
	passed: boolean;
	detail: string;
	remedy: string | null;
}

export interface DoctorReport {
	backend: BackendId;
	passed: boolean;
	checks: Check[];
}

export interface LaunchErrorBody {
	error: string;
	message: string;
	remedy: string | null;
}

export class LaunchApiError extends Error {
	constructor(
		message: string,
		readonly status: number,
		readonly code: string,
		readonly remedy: string | null = null
	) {
		super(message);
		this.name = 'LaunchApiError';
	}
}

async function launchGetJson<T>(path: string): Promise<T> {
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw await toLaunchApiError(res, path);
	}
	return (await res.json()) as T;
}

async function toLaunchApiError(res: Response, path: string): Promise<LaunchApiError> {
	let code = 'unknown';
	let message = `GET ${path} failed: ${res.status} ${res.statusText}`;
	let remedy: string | null = null;
	try {
		const body = (await res.json()) as Partial<LaunchErrorBody>;
		if (typeof body.error === 'string' && body.error.length > 0) code = body.error;
		if (typeof body.message === 'string' && body.message.length > 0) message = body.message;
		if (typeof body.remedy === 'string' && body.remedy.length > 0) remedy = body.remedy;
	} catch {}
	return new LaunchApiError(message, res.status, code, remedy);
}

export function getLaunches(run?: string): Promise<LaunchRow[]> {
	const path = run ? `/api/launches?run=${encodeURIComponent(run)}` : '/api/launches';
	return launchGetJson<LaunchRow[]>(path);
}

export function getLaunch(session: string): Promise<LaunchRow> {
	return launchGetJson<LaunchRow>(`/api/launches/${encodeURIComponent(session)}`);
}

export async function killLaunch(session: string): Promise<KillReport> {
	const path = `/api/launches/${encodeURIComponent(session)}`;
	const res = await fetch(path, { method: 'DELETE', headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw await toLaunchApiError(res, path);
	}
	return (await res.json()) as KillReport;
}

export function getLaunchDoctor(): Promise<DoctorReport> {
	return launchGetJson<DoctorReport>('/api/launches/doctor');
}
