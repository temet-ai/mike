/** Installed fleet onboarding using only the public, synthetic-session assignment API.
 * Paths are synthetic DB values, never filesystem paths to inspect or create.
 */
export const PROJECTS = ['/release-browser-fixture/alpha', '/release-browser-fixture/beta'];
export const ONBOARDING_CHECKS = ['fleet_empty_project_onboarding', 'fleet_single_project_selection',
  'fleet_multiple_project_deterministic_selection', 'fleet_explicit_project_reload'];
function requireValue(value) { if (!value) throw new Error('onboarding check failed'); }
export function validateProjects(rows, expected) {
  requireValue(Array.isArray(rows) && rows.length === expected.length &&
    rows.every(row => row && typeof row.project_path === 'string'));
  const actual = rows.map(row => row.project_path);
  requireValue(new Set(actual).size === actual.length &&
    JSON.stringify([...actual].sort()) === JSON.stringify([...expected].sort()));
  return actual;
}
export function fleetLocationMatches(raw, base, project) {
  try {
    const url = new URL(raw);
    return url.origin === base && url.pathname === '/fleet' && !url.username && !url.password && !url.hash &&
      url.searchParams.getAll('project').length === (project ? 1 : 0) &&
      (url.searchParams.get('project') ?? '') === project &&
      url.searchParams.getAll('window').length === 1 && url.searchParams.get('window') === '24';
  } catch { return false; }
}
export async function onboardingChecks(page, spec, helpers) {
  const { ownedHttp, waitFor, stage } = helpers;
  const base = `http://127.0.0.1:${spec.ports[2]}`;
  const rows = () => ownedHttp(spec.ports[1], '/api/projects').then(JSON.parse);
  const selected = page.locator('[data-slot="fleet-project-select"]');
  async function gotoFleet(url) {
    const response = await page.goto(url, { waitUntil: 'domcontentloaded' });
    const actual = new URL(page.url());
    requireValue(response?.status() === 200 && actual.origin === base && actual.pathname === '/fleet');
  }
  async function choose(url, expected) {
    const graph = page.waitForResponse(response => {
      const target = new URL(response.url());
      return target.origin === base && target.pathname === '/api/fleet/graph' &&
        target.searchParams.get('project') === expected;
    });
    await gotoFleet(url);
    requireValue((await graph).status() === 200);
    await waitFor(async () => await selected.inputValue() === expected && await selected.isEnabled() &&
      fleetLocationMatches(page.url(), base, expected));
    await waitFor(async () => await page.locator('[data-slot="fleet-graph-loading"]').count() === 0);
    requireValue(await page.locator('[data-slot="fleet-graph-error"]').count() === 0);
  }
  async function assign(project) {
    const value = JSON.parse(await ownedHttp(spec.ports[1], `/api/sessions/${spec.session}/project`,
      JSON.stringify({ project_path: project }), spec.session, 'PATCH'));
    requireValue(value.session_id === spec.session && value.project_path === project &&
      value.project_resolution === 'operator');
  }
  stage('BROWSER_ONBOARDING_EMPTY');
  validateProjects(await rows(), []);
  await gotoFleet(`${base}/fleet`);
  await page.getByRole('heading', { name: 'No projects captured yet', exact: true }).waitFor();
  requireValue(await selected.inputValue() === '' && await selected.isDisabled());
  await waitFor(() => fleetLocationMatches(page.url(), base, ''));
  await waitFor(async () => (await page.locator('[data-slot="empty-state-onboarding"]').innerText())
    .includes(`ANTHROPIC_BASE_URL='http://127.0.0.1:${spec.ports[0]}' claude`));
  stage('BROWSER_ONBOARDING_SINGLE');
  await assign(PROJECTS[1]);
  validateProjects(await rows(), [PROJECTS[1]]);
  await choose(`${base}/fleet`, PROJECTS[1]);
  stage('BROWSER_ONBOARDING_MULTIPLE');
  await assign(PROJECTS[0]);
  await assign(PROJECTS[1]);
  const order = validateProjects(await rows(), PROJECTS);
  requireValue(order[0] === PROJECTS[1]);
  await choose(`${base}/fleet`, PROJECTS[0]);
  stage('BROWSER_ONBOARDING_EXPLICIT');
  const explicit = `${base}/fleet?project=${encodeURIComponent(PROJECTS[1])}`;
  await choose(explicit, PROJECTS[1]);
  const response = await page.reload({ waitUntil: 'domcontentloaded' });
  requireValue(response?.status() === 200);
  await waitFor(async () => await selected.inputValue() === PROJECTS[1] &&
    fleetLocationMatches(page.url(), base, PROJECTS[1]));
  await page.goBack({ waitUntil: 'domcontentloaded' });
  await waitFor(async () => await selected.inputValue() === PROJECTS[0] &&
    fleetLocationMatches(page.url(), base, PROJECTS[0]));
}
