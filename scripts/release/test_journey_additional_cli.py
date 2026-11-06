"""Offline opt-in/report contracts for installed transition and upgrade helpers."""
from contextlib import contextmanager
import inspect
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import acceptance
from common import ReleaseError
from journey import new_result
from test_journey_faults import Offline


class AdditionalJourneyCliTests(Offline):
    base = ['release-acceptance', '--candidate', '/tmp/candidate', '--output', '/tmp/evidence']

    def test_new_journeys_remain_keyword_only_and_opt_in(self):
        for function in (acceptance.run_helpers, acceptance.run_acceptance):
            parameters = inspect.signature(function).parameters
            for name, default in [('mode_journey', False), ('mode_tui_journey', False),
                                  ('upgrade_predecessor', None)]:
                self.assertEqual(parameters[name].kind, inspect.Parameter.KEYWORD_ONLY)
                self.assertIs(parameters[name].default, default)
        with patch.object(sys, 'argv', self.base), patch.object(acceptance, 'run_acceptance', return_value=2) as run:
            self.assertEqual(acceptance.main(), 2)
        self.assertFalse(run.call_args.kwargs['mode_journey'])
        self.assertFalse(run.call_args.kwargs['mode_tui_journey'])
        self.assertIsNone(run.call_args.kwargs['upgrade_predecessor'])

    def test_explicit_modes_and_predecessor_are_forwarded(self):
        flags = ['--mode-journey', '--mode-tui-journey', '--upgrade-predecessor', '/tmp/predecessor']
        with patch.object(sys, 'argv', self.base + flags), patch.object(acceptance, 'run_acceptance', return_value=2) as run:
            self.assertEqual(acceptance.main(), 2)
        self.assertTrue(run.call_args.kwargs['mode_journey'])
        self.assertTrue(run.call_args.kwargs['mode_tui_journey'])
        self.assertEqual(run.call_args.kwargs['upgrade_predecessor'], Path('/tmp/predecessor'))

    def test_invalid_dependencies_do_not_start_helpers(self):
        for flags in [['--mode-tui-journey'], ['--browser-ipv6']]:
            with self.subTest(flags=flags), patch.object(sys, 'argv', self.base + flags), \
                    patch.object(acceptance, 'run_acceptance') as run, self.assertRaises(SystemExit) as caught:
                acceptance.main()
            self.assertEqual(caught.exception.code, 2)
            run.assert_not_called()
        for function in (acceptance.run_helpers, acceptance.run_acceptance):
            with self.assertRaisesRegex(ReleaseError, 'requires --mode-journey'):
                function(Path('/unused/candidate'), Path('/unused/output'), mode_tui_journey=True)

    def test_ipv6_requires_explicit_browser_and_is_forwarded(self):
        flags = ['--persistent-journey', '--browser-journey', '--browser-node', '/trusted/node',
                 '--browser-chromium', '/trusted/chromium', '--browser-ipv6']
        with patch.object(sys, 'argv', self.base + flags), patch.object(acceptance, 'BrowserTools') as tools, \
                patch.object(acceptance, 'run_acceptance', return_value=2):
            self.assertEqual(acceptance.main(), 2)
        tools.assert_called_once_with(Path('/trusted/node'), Path('/trusted/chromium'), ipv6=True)

    def test_evidence_cannot_mutate_predecessor_input_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            predecessor = root / 'predecessor'
            predecessor.mkdir()
            output = predecessor / 'evidence'
            with self.assertRaisesRegex(ReleaseError, 'outside'):
                acceptance.run_acceptance(root / 'candidate', output, upgrade_predecessor=predecessor)
            self.assertFalse(output.exists())

    @contextmanager
    def fixture(self, failure=None):
        manifest = {'version': '0.1.0', 'source_commit': 'a' * 40, 'target': 'offline',
                    'artifacts': [{'binary': name, 'archive': name + '.tar.gz',
                                   'sha256': 'b' * 64, 'smoke': {}} for name in ('mike', 'mike-mcp')]}
        @contextmanager
        def verified(_):
            yield Path('/offline/snapshot'), manifest
        calls = []
        def mode(binary, actual, sandbox, digest, *, tui):
            self.assertEqual(binary.name, 'bin')
            self.assertEqual(actual, manifest)
            self.assertEqual(digest, 'c' * 64)
            calls.append(('mode', tui))
            result = new_result({})
            if failure == 'mode': result['state'] = 'FAILED'
            return result
        def upgrade(snapshot, actual, sandbox, digest, predecessor):
            self.assertEqual(snapshot, Path('/offline/snapshot'))
            self.assertEqual(actual, manifest)
            self.assertEqual(digest, 'c' * 64)
            self.assertEqual(predecessor, Path('/offline/predecessor'))
            self.assertTrue(sandbox.name.startswith('mike-acceptance-'))
            calls.append(('upgrade', sandbox))
            result = new_result({})
            if failure == 'upgrade': result['state'] = 'FAILED'
            return result
        with patch.object(acceptance, 'verified_inputs', verified), \
                patch.object(acceptance, 'install_verified', return_value={'status': 'unchanged'}), \
                patch.object(acceptance, 'smoke', return_value={}), \
                patch.object(acceptance, 'sha256', return_value='c' * 64), \
                patch.object(acceptance, 'run_mode_journey', side_effect=mode), \
                patch.object(acceptance, 'run_upgrade_journey', side_effect=upgrade):
            yield calls

    def test_separate_helpers_record_partial_evidence_without_promoting_cases(self):
        report = acceptance.new_report()
        with self.fixture() as calls:
            acceptance.run_helpers(Path('/offline/candidate'), report, mode_journey=True,
                                   mode_tui_journey=True, upgrade_predecessor=Path('/offline/predecessor'))
        self.assertEqual([call[0] for call in calls], ['mode', 'upgrade'])
        self.assertEqual(calls[0], ('mode', True))
        self.assertFalse(calls[1][1].exists())
        self.assertIn('archive_mode_journey', report)
        self.assertIn('archive_upgrade_journey', report)
        self.assertFalse(report['qualified'])
        self.assertTrue(all(case['state'] == 'NOT_RUN' for case in report['cases']))

    def test_failed_helper_stops_later_work(self):
        for failure in ('mode', 'upgrade'):
            report = acceptance.new_report()
            with self.subTest(failure=failure), self.fixture(failure) as calls, self.assertRaises(ReleaseError):
                acceptance.run_helpers(Path('/offline/candidate'), report, mode_journey=True,
                                       upgrade_predecessor=Path('/offline/predecessor'))
            self.assertEqual(len(calls), 1 if failure == 'mode' else 2)
            self.assertEqual(report['archive_' + failure + '_journey']['state'], 'FAILED')


class ReliabilityObservationCliTests(Offline):
    budgets = {'rss_peak_bytes': 104857600, 'fd_peak': 256, 'broadcast_backlog_peak': 0,
               'active_streams_peak': 8, 'drain_seconds': 5, 'responsiveness_seconds': 2}

    def test_profiles_and_budgets_require_the_reliability_journey(self):
        for profile, budgets in [('normal_defaults', self.budgets),
                                 ('synthetic_isolation_overrides', self.budgets)]:
            with self.assertRaisesRegex(ReleaseError, 'require --reliability-journey'):
                acceptance.requested_observations(False, profile, budgets)
        self.assertEqual(acceptance.requested_observations(False, 'synthetic_isolation_overrides', None),
                         {'profile': 'synthetic_isolation_overrides', 'resource_budgets': None})

    def test_budget_file_is_bounded_regular_unique_json(self):
        import json
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / 'budgets.json'
            path.write_text(json.dumps(self.budgets))
            self.assertEqual(acceptance.resource_budget_file(path), self.budgets)
            alias = root / 'alias.json'
            alias.symlink_to(path)
            with self.assertRaises(ReleaseError): acceptance.resource_budget_file(alias)
            for payload in ('{"fd_peak":1,"fd_peak":2}', '{"rss_peak_bytes":NaN}', ' ' * 16385,
                            json.dumps(dict(self.budgets, fd_peak=True)),
                            json.dumps(dict(self.budgets, drain_seconds=31))):
                path.write_text(payload)
                with self.assertRaises(ReleaseError): acceptance.resource_budget_file(path)

    def test_explicit_budgets_and_profile_are_forwarded_without_defaults(self):
        import json
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'budgets.json'
            path.write_text(json.dumps(self.budgets))
            flags = ['--reliability-journey', '--reliability-profile', 'normal_defaults',
                     '--reliability-resource-budgets', str(path)]
            with patch.object(sys, 'argv', AdditionalJourneyCliTests.base + flags), \
                    patch.object(acceptance, 'run_acceptance', return_value=2) as run:
                self.assertEqual(acceptance.main(), 2)
            self.assertEqual(run.call_args.kwargs['reliability_profile'], 'normal_defaults')
            self.assertEqual(run.call_args.kwargs['reliability_resource_budgets'], self.budgets)

    def test_normal_profile_without_budgets_never_starts_helpers(self):
        flags = ['--reliability-journey', '--reliability-profile', 'normal_defaults']
        with patch.object(sys, 'argv', AdditionalJourneyCliTests.base + flags), \
                patch.object(acceptance, 'run_acceptance') as run, self.assertRaises(SystemExit) as caught:
            acceptance.main()
        self.assertEqual(caught.exception.code, 2)
        run.assert_not_called()


if __name__ == '__main__':
    unittest.main()
