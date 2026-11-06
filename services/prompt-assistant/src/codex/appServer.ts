import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { createInterface } from 'node:readline';

type JsonObject = Record<string, unknown>;
type NotificationHandler = (message: JsonObject) => void | Promise<void>;
type ServerRequestHandler = (method: string, params: JsonObject) => Promise<unknown>;

export interface CodexAccountStatus {
	installed: boolean;
	logged_in: boolean;
	account_type: string | null;
	plan_type: string | null;
	email: string | null;
	rate_limits: unknown;
	usage: unknown;
	error: string | null;
}

export class CodexAppServer {
	private child: ChildProcessWithoutNullStreams;
	private nextId = 1;
	private pending = new Map<
		number,
		{ resolve: (value: unknown) => void; reject: (error: Error) => void }
	>();
	private notificationHandler: NotificationHandler | null = null;
	private serverRequestHandler: ServerRequestHandler | null = null;
	private stderr = '';

	private constructor(child: ChildProcessWithoutNullStreams) {
		this.child = child;
		createInterface({ input: child.stdout }).on('line', (line) => this.receive(line));
		child.stderr.on('data', (chunk: Buffer) => {
			this.stderr = `${this.stderr}${chunk.toString('utf8')}`.slice(-4000);
		});
		child.on('exit', (code) => {
			const detail = this.stderr.trim();
			const error = new Error(detail || `codex app-server exited with code ${code ?? 'unknown'}`);
			for (const pending of this.pending.values()) pending.reject(error);
			this.pending.clear();
			void this.notificationHandler?.({
				method: 'transport/closed',
				params: { message: error.message }
			});
		});
		child.on('error', (cause) => {
			const error = new Error(`could not start codex app-server: ${cause.message}`);
			for (const pending of this.pending.values()) pending.reject(error);
			this.pending.clear();
		});
	}

	static async start(): Promise<CodexAppServer> {
		const child = spawn('codex', ['app-server'], {
			stdio: ['pipe', 'pipe', 'pipe']
		});
		const server = new CodexAppServer(child);
		await server.request('initialize', {
			clientInfo: {
				name: 'mike_prompt_assistant',
				title: 'Mike prompt assistant',
				version: '0.1.0'
			},
			capabilities: { experimentalApi: true, requestAttestation: false }
		});
		server.notify('initialized', {});
		return server;
	}

	onNotification(handler: NotificationHandler): void {
		this.notificationHandler = handler;
	}

	onServerRequest(handler: ServerRequestHandler): void {
		this.serverRequestHandler = handler;
	}

	request(method: string, params: JsonObject = {}): Promise<unknown> {
		const id = this.nextId++;
		return new Promise((resolve, reject) => {
			this.pending.set(id, { resolve, reject });
			this.write({ method, id, params });
		});
	}

	notify(method: string, params: JsonObject): void {
		this.write({ method, params });
	}

	close(): void {
		this.child.stdin.end();
		if (!this.child.killed) this.child.kill('SIGTERM');
	}

	private write(message: JsonObject): void {
		this.child.stdin.write(`${JSON.stringify(message)}\n`);
	}

	private receive(line: string): void {
		let message: JsonObject;
		try {
			message = JSON.parse(line) as JsonObject;
		} catch {
			return;
		}
		const id = message.id;
		if (typeof id === 'number' && typeof message.method === 'string') {
			void this.answerServerRequest(id, message.method, (message.params as JsonObject) ?? {});
			return;
		}
		if (typeof id === 'number') {
			const pending = this.pending.get(id);
			if (!pending) return;
			this.pending.delete(id);
			if (message.error) pending.reject(new Error(this.errorMessage(message.error)));
			else pending.resolve(message.result);
			return;
		}
		if (typeof message.method === 'string') void this.notificationHandler?.(message);
	}

	private async answerServerRequest(id: number, method: string, params: JsonObject): Promise<void> {
		try {
			if (!this.serverRequestHandler) throw new Error(`unsupported Codex request: ${method}`);
			const result = await this.serverRequestHandler(method, params);
			this.write({ id, result });
		} catch (error) {
			this.write({
				id,
				error: { code: -32000, message: (error as Error).message }
			});
		}
	}

	private errorMessage(value: unknown): string {
		if (typeof value === 'object' && value !== null && 'message' in value) {
			return String((value as { message: unknown }).message);
		}
		return String(value);
	}
}

export async function readCodexAccountStatus(): Promise<CodexAccountStatus> {
	let server: CodexAppServer | null = null;
	try {
		server = await CodexAppServer.start();
		const accountResult = await server.request('account/read', {
			refreshToken: false
		});
		const [rateLimitsResult, usageResult] = await Promise.allSettled([
			server.request('account/rateLimits/read', {
				excludeResetCreditDetails: true
			}),
			server.request('account/usage/read', {})
		]);
		const account = (accountResult as { account?: JsonObject | null }).account ?? null;
		const failures = [rateLimitsResult, usageResult]
			.filter((result): result is PromiseRejectedResult => result.status === 'rejected')
			.map((result) =>
				result.reason instanceof Error ? result.reason.message : String(result.reason)
			);
		return {
			installed: true,
			logged_in: account !== null,
			account_type: typeof account?.type === 'string' ? account.type : null,
			plan_type: typeof account?.planType === 'string' ? account.planType : null,
			email: typeof account?.email === 'string' ? account.email : null,
			rate_limits: rateLimitsResult.status === 'fulfilled' ? rateLimitsResult.value : null,
			usage: usageResult.status === 'fulfilled' ? usageResult.value : null,
			error: failures.length > 0 ? failures.join('; ') : null
		};
	} catch (error) {
		const message = (error as Error).message;
		return {
			installed: !message.includes('ENOENT'),
			logged_in: false,
			account_type: null,
			plan_type: null,
			email: null,
			rate_limits: null,
			usage: null,
			error: message
		};
	} finally {
		server?.close();
	}
}
