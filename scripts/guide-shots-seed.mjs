// The guide's fictional example and how it gets into a scratch mike (see scripts/guide-shots).
// Everything is invented: the folder, the session, the correction and the reply texts.

export const DEMO = {
  folder: '/home/demo/projects/site',
  session: '5b6c2a1e-3f4d-4a8e-9c71-0d2e8f4a6b10',
  peer: '8e1f7d52-6a0b-4c3e-b2d9-4f5a9c1e7a33',
  name: 'docs',
  role: 'writer',
  goal: 'Rewrite the install page',
  correction: 'Put Homebrew first on the install page.',
  replies: [
    { text: 'I will start with the install page. It lists the archive download first, then Homebrew, then mise. I will read it and draft a shorter version.', tool: { id: 'toolu_demo1', name: 'Read', input: { file_path: '/home/demo/projects/site/content/install.md' } } },
    { text: 'The page now opens with a short summary and the three ways to install. The archive download comes first because it works on every platform.' },
    { text: 'Understood. I have moved Homebrew to the top of the install page and kept the archive download and mise below it as alternatives.', tool: { id: 'toolu_demo2', name: 'Edit', input: { file_path: '/home/demo/projects/site/content/install.md', old_string: '## Archive', new_string: '## Homebrew' } } },
    { text: 'Homebrew is first now, and the page links to the archive for platforms without it. I left the mise section as it is for now.' },
  ],
};

const SYSTEM = (folder) => [
  { type: 'text', text: 'x-anthropic-billing-header: cc_version=2.1.0.a1b; cc_entrypoint=cli;' },
  { type: 'text', text: "You are Claude Code, Anthropic's official CLI for Claude." },
  { type: 'text', text: `You help with software and writing tasks.\n\nEnvironment\nWorking directory: ${folder}\nIs a git repository: true\nPlatform: linux\n\nAlways finish with a long summary of every change.\n` },
];

async function postJson(url, body, headers = {}) {
  const r = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json', ...headers }, body: JSON.stringify(body) });
  const text = await r.text();
  if (!r.ok) throw new Error(`${url} -> ${r.status} ${text.slice(0, 300)}`);
  return text;
}

export async function sendTurn(ctx, session, messages) {
  const text = await postJson(`${ctx.proxy}/v1/messages`, {
    model: 'claude-sonnet-4-5', max_tokens: 4096, stream: true, system: SYSTEM(DEMO.folder), messages,
  }, { 'x-claude-code-session-id': session, 'user-agent': 'claude-cli/2.1.0 (external, cli)', 'anthropic-version': '2023-06-01' });
  return text;
}

export async function seedDemo(ctx) {
  const reg = (id, name, role, goal) => postJson(`${ctx.inspect}/api/workspace/register`, { session_id: id, cwd: DEMO.folder, registered_via: 'passive', name, role, goal });
  await reg(DEMO.session, DEMO.name, DEMO.role, DEMO.goal);
  await reg(DEMO.peer, 'review', 'reviewer', 'Review the install page');
  await call(ctx, 'POST', '/api/filter/rules', { kind: 'text_substring', value: 'Always finish with a long summary of every change.', action: 'replace', text: 'Finish with a one-line summary.', title: 'Shorter summaries' });
  const h = [];
  const user = (t) => ({ role: 'user', content: [{ type: 'text', text: t }] });
  const asst = (t, tool) => ({ role: 'assistant', content: [{ type: 'text', text: t }, ...(tool ? [{ type: 'tool_use', id: tool.id, name: tool.name, input: tool.input }] : [])] });
  const toolResult = (id, t) => ({ role: 'user', content: [{ type: 'tool_result', tool_use_id: id, content: t }] });
  const R = DEMO.replies;
  h.push(user(`${DEMO.goal}. Keep it short and put the most common path first.`));
  await sendTurn(ctx, DEMO.session, [...h]);
  h.push(asst(R[0].text, R[0].tool), toolResult('toolu_demo1', '# Install\n\n## Archive\nDownload the archive for your platform.\n\n## Homebrew\n`brew install demo`\n\n## mise\n`mise use demo`\n'), );
  await sendTurn(ctx, DEMO.session, [...h]);
  h.push(asst(R[1].text), user(DEMO.correction));
  await sendTurn(ctx, DEMO.session, [...h]);
  h.push(asst(R[2].text, R[2].tool), toolResult('toolu_demo2', 'The file has been updated.'));
  await sendTurn(ctx, DEMO.session, [...h]);
  await new Promise((r) => setTimeout(r, 1500));
  // Put the sessions under their project so they are not listed as Ungrouped.
  for (const id of [DEMO.session, DEMO.peer]) await call(ctx, 'PATCH', `/api/sessions/${id}/project`, { project_path: DEMO.folder });
  const turns = await call(ctx, 'GET', `/api/operator-turns?session_id=${DEMO.session}&limit=10`);
  const turnId = (turns.find?.((t) => t.user_prompt === DEMO.correction) ?? {}).turn_id;
  await call(ctx, 'POST', '/api/lessons', { project_path: DEMO.folder, kind: 'correction', session_id: DEMO.session, source_turn_id: turnId, text: 'On the install page, list Homebrew first; the archive download and mise follow as alternatives.' });
  await call(ctx, 'POST', '/api/lessons', { project_path: DEMO.folder, kind: 'convention', session_id: DEMO.session, text: 'Give each install method one command and one sentence on when to use it.' });
  const reqs = (await call(ctx, 'GET', `/api/sessions/${DEMO.session}/requests`)).requests;
  const second = reqs.map((r) => r.id).sort((a, b) => a - b)[1];
  await call(ctx, 'POST', '/api/compression-review', { kind: 'request', request_id: second, reason: 'The whole install page came back as one tool result.', evidence: 'Request 2 carries the full text of content/install.md.', actor: 'operator' });
  const steerers = await call(ctx, 'GET', '/api/steerers');
  const refocus = steerers.find((x) => x.name === 'refocus');
  await call(ctx, 'PATCH', `/api/steerers/${refocus.id}`, { enabled: true });
  await call(ctx, 'POST', `/api/sessions/${DEMO.session}/enrolments`, { steerer_id: refocus.id, mode: 'drive', authority: 'command' });
  const thread = await call(ctx, 'POST', '/api/threads', { subject: 'Install page: which method first?', body: 'Most readers install with Homebrew. Can the page open with it?', from: DEMO.peer, project_path: DEMO.folder, tags: ['install'], notify: [DEMO.session] });
  await call(ctx, 'POST', `/api/threads/${thread.thread_uuid}/message`, { from: DEMO.session, body: 'Yes. The correction is in: Homebrew is first, the archive and mise follow.', to: [DEMO.peer] });
}

export async function call(ctx, method, path, body) {
  const r = await fetch(`${ctx.inspect}${path}`, { method, headers: { 'content-type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body) });
  const text = await r.text();
  if (!r.ok) throw new Error(`${method} ${path} -> ${r.status} ${text.slice(0, 300)}`);
  try { return JSON.parse(text); } catch { return text; }
}
