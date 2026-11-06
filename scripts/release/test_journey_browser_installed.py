"""Offline orchestration of a separate persistent browser fixture. No DB/browser launch."""
from contextlib import ExitStack, contextmanager
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch

import acceptance
import journey_browser_installed as installed
from journey import new_result
from journey_browser import BrowserTools
from journey_fixture import Fixture
from journey_process import JourneyError
from test_journey_persistent import synthetic_inventory


class InstalledBrowserTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="mike-browser-persistent-offline-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.tools = BrowserTools(Path('/offline/node'), Path('/offline/chromium'))
        self.paths = {name: self.root / name for name in ('mike', 'mike-mcp')}
        self.binding = {'manifest_sha256': 'a' * 64}
        self.manifest = {'source_commit': 'b' * 40, 'version': '0.0.1'}
        self.inventory = synthetic_inventory()

    @contextmanager
    def fixtures(self):
        cluster = Mock()
        cluster.create.return_value = 'separate_browser_db'
        cluster.sql.return_value = '0'
        cluster.config_url.return_value = 'postgres://postgres@127.0.0.1:6432/separate_browser_db?sslmode=disable'
        fixture = Fixture()
        order = []
        starts = []
        @contextmanager
        def child(args, cwd, environment):
            starts.append((args, cwd, environment))
            order.append('child_start')
            try:
                yield Mock()
            finally:
                order.append('child_cleanup')
        @contextmanager
        def upstream():
            yield fixture, 11004
        @contextmanager
        def ports():
            yield [11001, 11002, 11003], [Mock(), Mock(), Mock()]
        cluster.cleanup.side_effect = lambda: order.append('database_cleanup')
        ledger = {'full_ledger_sha256': 'c' * 64, 'migration_count': 2}
        with ExitStack() as stack:
            definitions = {
                'verify_binding': {'return_value': (self.paths, self.binding)},
                'browser_tools': {'return_value': self.tools},
                'verified_migration_inventory': {'return_value': self.inventory},
                'owned_cluster': {'return_value': cluster},
                'owned_child': {'side_effect': child}, 'upstream_fixture': {'side_effect': upstream},
                'reserved_ports': {'side_effect': ports}, 'wait_owned_ready': {'return_value': {}},
                'doctor': {'return_value': {}}, 'stream_capture': {'return_value': {}},
                'migration_ledger': {'return_value': ledger},
                'stored_capture': {'return_value': {'request_id': 42, 'session_id': fixture.session}},
                'persisted_session': {'return_value': {'session_list_and_detail': True}},
                'spa_http': {'return_value': {}}, 'run_browser': {'return_value': {'qualified': False}},
                'run_browser_security': {'return_value': {'qualified': False}},
                'run_ipv6_boundary': {'side_effect': lambda *_args: order.append('ipv6_boundary') or {}},
                'graceful_stop': {'side_effect': lambda _child: order.append('graceful_stop') or {}},
            }
            mocks = {name: stack.enter_context(patch.object(installed, name, **options))
                     for name, options in definitions.items()}
            yield cluster, fixture, order, starts, mocks

    def run_journey(self):
        return installed.run_installed_browser_journey(self.root, self.manifest, self.root, 'a' * 64, self.tools)

    def test_browser_has_fresh_database_persistent_config_and_independent_owned_cleanup(self):
        with self.fixtures() as (cluster, fixture, order, starts, mocks):
            report = self.run_journey()
            cluster.create.assert_called_once_with(seeded=False)
            cluster.config_url.assert_called_once_with('separate_browser_db')
            cluster.cleanup.assert_called_once_with()
            self.assertEqual(len(starts), 1)
            args, cwd, env = starts[0]
            config = Path(args[2])
            self.assertIn('persist_enabled = true', config.read_text())
            self.assertIn('/separate_browser_db?', config.read_text())
            self.assertEqual(cwd.parent.name, 'browser')
            self.assertEqual(env['PATH'], '')
            self.assertEqual(order, ['child_start', 'graceful_stop', 'child_cleanup', 'database_cleanup'])
            self.assertEqual(mocks['wait_owned_ready'].call_count, 5)
            self.assertTrue(all(call.kwargs['persistent'] for call in mocks['wait_owned_ready'].call_args_list))
            mocks['stored_capture'].assert_called_once_with(11002, fixture)
            self.assertEqual([call.args[-1] for call in mocks['persisted_session'].call_args_list], [1, 2, 3])
            mocks['run_browser'].assert_called_once_with(self.tools, [11001, 11002, 11003], fixture.session,
                                                       42, fixture, config.parent / 'home', cwd)
            self.assertEqual(mocks['verify_binding'].call_count, 3)
            self.assertEqual(mocks['verified_migration_inventory'].call_count, 3)
        self.assertEqual(report['state'], 'INCOMPLETE')
        self.assertFalse(report['qualified'])
        self.assertTrue(all(case['state'] == 'NOT_RUN' for case in report['full_cases']))
        self.assertTrue(all(item['binding'] == self.binding for item in report['checks']))

    def test_ipv6_is_opt_in_and_runs_only_after_ipv4_owned_stop(self):
        with self.fixtures() as (_cluster, _fixture, order, _starts, mocks):
            report = self.run_journey()
            mocks['run_ipv6_boundary'].assert_not_called()
            row = next(item for item in report['checks'] if item['id'] == 'archive_browser_ipv6_native_boundary')
            self.assertEqual(row['state'], 'NOT_RUN')
        self.root = self.root / 'second-run'
        self.root.mkdir()
        with self.fixtures() as (_cluster, _fixture, order, _starts, mocks):
            self.tools = BrowserTools(self.tools.node, self.tools.chromium, ipv6=True)
            mocks['browser_tools'].return_value = self.tools
            report = self.run_journey()
            mocks['run_ipv6_boundary'].assert_called_once()
            self.assertLess(order.index('child_cleanup'), order.index('ipv6_boundary'))
            self.assertLess(order.index('ipv6_boundary'), order.index('database_cleanup'))
            self.assertEqual(report['state'], 'INCOMPLETE')

    def test_tooling_hash_source_or_cluster_failure_never_allocates_or_launches(self):
        for name in ('verify_binding', 'browser_tools', 'verified_migration_inventory', 'owned_cluster'):
            with self.subTest(name=name), self.fixtures() as (cluster, _fixture, _order, starts, mocks):
                mocks[name].side_effect = JourneyError('SYNTHETIC_PREFLIGHT_FAILURE')
                report = self.run_journey()
                cluster.create.assert_not_called()
                self.assertFalse(starts)
                mocks['run_browser'].assert_not_called()
                self.assertEqual(report['state'], 'FAILED')

    def test_nonempty_database_is_not_used_and_is_cleaned(self):
        with self.fixtures() as (cluster, _fixture, order, starts, mocks):
            cluster.sql.return_value = '1'
            report = self.run_journey()
            self.assertFalse(starts)
            mocks['run_browser'].assert_not_called()
            cluster.cleanup.assert_called_once_with()
        self.assertEqual(report['failure_code'], 'BROWSER_FIXTURE_NOT_EMPTY')
        self.assertEqual(order, ['database_cleanup'])

    def test_browser_failure_cleans_owned_child_before_only_its_database(self):
        with self.fixtures() as (cluster, _fixture, order, _starts, mocks):
            mocks['run_browser'].side_effect = JourneyError('BROWSER_CAPTURE_DOM')
            report = self.run_journey()
            cluster.cleanup.assert_called_once_with()
            mocks['graceful_stop'].assert_not_called()
        self.assertEqual(order, ['child_start', 'child_cleanup', 'database_cleanup'])
        self.assertEqual(report['failure_code'], 'BROWSER_CAPTURE_DOM')
        self.assertFalse(report['qualified'])

    def test_cleanup_and_private_failures_cannot_be_reported_as_success(self):
        for phase in ('browser', 'cleanup', 'ledger'):
            with self.subTest(phase=phase), self.fixtures() as (cluster, _fixture, _order, _starts, mocks):
                if phase == 'browser':
                    mocks['run_browser'].side_effect = ValueError('PRIVATE_DATABASE_URL')
                elif phase == 'cleanup':
                    cluster.cleanup.side_effect = ValueError('PRIVATE_DATABASE_URL')
                else:
                    mocks['migration_ledger'].side_effect = [{'full_ledger_sha256': 'a'}, {'full_ledger_sha256': 'b'}]
                report = self.run_journey()
                self.assertEqual(report['state'], 'FAILED')
                self.assertNotIn('PRIVATE_DATABASE_URL', json.dumps(report))

    def test_missing_persisted_session_or_body_never_launches_browser(self):
        for name in ('stored_capture', 'persisted_session'):
            with self.subTest(name=name), self.fixtures() as (_cluster, _fixture, _order, _starts, mocks):
                mocks[name].side_effect = JourneyError('SYNTHETIC_MISSING_PERSISTENCE')
                report = self.run_journey()
                mocks['run_browser'].assert_not_called()
                self.assertEqual(report['state'], 'FAILED')


class PersistedSessionTests(unittest.TestCase):
    def responses(self, count=1):
        rows = [{'id': 'session', 'request_count': count}]
        detail = {'summary': rows[0], 'requests': [{'id': value} for value in range(42, 42 + count)]}
        return [rows, detail]

    def test_exact_persisted_session_apis_accept_initial_and_live_counts(self):
        for count in (1, 2, 3):
            with self.subTest(count=count), patch.object(installed, 'http_json', side_effect=self.responses(count)) as read:
                result = installed.persisted_session(11002, 'session', 42, count)
                self.assertEqual(result['request_count'], count)
                self.assertEqual([call.args[1] for call in read.call_args_list], ['/api/sessions', '/api/sessions/session'])

    def test_missing_trial_session_times_out_instead_of_claiming_dom_ready(self):
        with patch.object(installed, 'http_json', return_value=[]), \
                patch.object(installed.time, 'monotonic', side_effect=[0, 0, 2]), \
                patch.object(installed.time, 'sleep'):
            with self.assertRaisesRegex(JourneyError, 'PERSISTED_SESSION_TIMEOUT'):
                installed.persisted_session(11002, 'session', 42, 1, timeout=1)

    def test_foreign_duplicate_or_malformed_session_is_refused(self):
        bad = [[{'id': 'foreign', 'request_count': 1}], [{'id': 'session'}, {'id': 'foreign'}], 'wrong']
        for rows in bad:
            with self.subTest(rows=rows), patch.object(installed, 'http_json', return_value=rows):
                with self.assertRaises(JourneyError):
                    installed.persisted_session(11002, 'session', 42, 1)
        values = self.responses(2)
        values[1]['requests'] = [{'id': 42}, {'id': 42}]
        with patch.object(installed, 'http_json', side_effect=values):
            with self.assertRaisesRegex(JourneyError, 'DUPLICATED'):
                installed.persisted_session(11002, 'session', 42, 2)


class BrowserAcceptanceOrderTests(unittest.TestCase):
    def test_separate_browser_runs_after_baseline_persistent_cleanup(self):
        manifest = {'version': '0.0.1', 'source_commit': 'a' * 40, 'target': 'offline',
                    'artifacts': [{'binary': name, 'archive': name + '.tar.gz', 'sha256': 'b' * 64, 'smoke': {}}
                                  for name in ('mike', 'mike-mcp')]}
        order = []
        @contextmanager
        def verified(_candidate):
            yield Path('/offline/snapshot'), manifest
        def baseline(*_args, **options):
            self.assertEqual(options, {'maintenance': True})
            order.extend(['persistent', 'maintenance', 'baseline_cleanup'])
            return new_result({})
        def browser(*_args):
            self.assertEqual(order[-1], 'baseline_cleanup')
            order.append('separate_browser')
            return new_result({})
        with patch.object(acceptance, 'verified_inputs', side_effect=verified), \
                patch.object(acceptance, 'sha256', return_value='c' * 64), \
                patch.object(acceptance, 'install_verified', return_value={'status': 'unchanged'}), \
                patch.object(acceptance, 'smoke', return_value={}), \
                patch.object(acceptance, 'run_archive_journey') as trial, \
                patch.object(acceptance, 'run_persistent_journey', side_effect=baseline), \
                patch.object(acceptance, 'run_installed_browser_journey', side_effect=browser):
            report = acceptance.new_report()
            acceptance.run_helpers(Path('/offline/candidate'), report, persistent_journey=True,
                                   browser=BrowserTools(Path('/offline/node'), Path('/offline/chromium')), maintenance=True)
            trial.assert_not_called()
        self.assertEqual(order, ['persistent', 'maintenance', 'baseline_cleanup', 'separate_browser'])
        self.assertIn('archive_browser_journey', report)
        self.assertTrue(all(case['state'] == 'NOT_RUN' for case in report['cases']))


if __name__ == '__main__':
    unittest.main(verbosity=2)
