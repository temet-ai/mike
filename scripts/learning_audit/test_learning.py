import argparse
import json
import os
from datetime import date, timedelta
import hashlib
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from learning_audit.report import findings, opportunity
from tool_reuse import core


class ReuseTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.state = patch.object(core, 'STATE', self.root / 'state')
        self.state.start()
        (self.root / 'input').write_text('one')

    def tearDown(self):
        self.state.stop()
        self.tmp.cleanup()

    def recipe(self, command=None):
        args = argparse.Namespace(id='read-input', description='Read a fixture', cwd=str(self.root),
            command=command or ['/bin/cat', 'input'], dependency=['input'], environment=[],
            ttl=3600, contract='Only reads declared input; cached until content changes')
        return core.register(args)

    def test_reuse_and_content_invalidation(self):
        recipe = self.recipe()
        first = core.run(recipe)
        self.assertFalse(first['reused'])
        self.assertTrue(core.run(recipe)['reused'])
        (self.root / 'input').write_text('two')
        changed = core.run(recipe)
        self.assertFalse(changed['reused'])
        self.assertEqual(Path(changed['stdout']).read_text(), 'two')
        self.assertEqual(Path(first['stdout']).read_text(), 'one')

    def test_failure_is_preserved_and_retried(self):
        recipe = self.recipe([shutil.which('false')])
        first, second = core.run(recipe), core.run(recipe)
        self.assertEqual(first['exit_code'], 1)
        self.assertFalse(second['reused'])

    def test_environment_and_working_directory_affect_identity(self):
        recipe = self.recipe()
        first = core.identity(recipe)
        with patch.dict(os.environ, {'REUSE_CONTEXT_TEST': 'different'}):
            self.assertNotEqual(first, core.identity(recipe))
        other = self.root / 'other'
        other.mkdir(); (other / 'input').write_text('one')
        self.assertNotEqual(first, core.identity({**recipe, 'cwd': str(other)}))

    def test_no_shell_evaluation_and_invalid_id_rejected(self):
        with self.assertRaises(ValueError):
            core.valid_id('../escape')
        recipe = self.recipe(['/bin/echo', '$(touch unwanted)'])
        result = core.run(recipe)
        self.assertEqual(Path(result['stdout']).read_text(), '$(touch unwanted)\n')
        self.assertFalse((self.root / 'unwanted').exists())

    def test_dependency_change_during_command_not_reused(self):
        recipe = self.recipe(['/usr/bin/python3', '-c', "open('input','w').write('changed')"])
        result = core.run(recipe)
        self.assertFalse(result['dependencies_unchanged'])
        self.assertFalse((Path(result['stdout']).parent.parent / 'result.json').exists())

    def test_fresh_runs_keep_prior_artifacts_immutable(self):
        recipe = self.recipe(['/usr/bin/python3', '-c', 'import time; print(time.time_ns())'])
        first = core.run(recipe)
        original = Path(first['stdout']).read_bytes()
        second = core.run(recipe, force=True)
        self.assertNotEqual(first['stdout'], second['stdout'])
        self.assertEqual(Path(first['stdout']).read_bytes(), original)
        self.assertNotEqual(Path(second['stdout']).read_bytes(), original)

    def test_change_while_waiting_for_lock_recomputes_identity(self):
        recipe = self.recipe()
        old = core.run(recipe)
        real_flock = core.fcntl.flock
        changed = False
        def change_before_acquisition(file, operation):
            nonlocal changed
            if not changed:
                changed = True
                (self.root / 'input').write_text('changed while waiting')
            return real_flock(file, operation)
        with patch.object(core.fcntl, 'flock', side_effect=change_before_acquisition):
            new = core.run(recipe)
        self.assertFalse(new['reused'])
        self.assertNotEqual(old['key'], new['key'])
        self.assertEqual(Path(new['stdout']).read_text(), 'changed while waiting')

    def test_symlink_directory_dependency_is_rejected(self):
        directory = self.root / 'directory'
        directory.mkdir(); (directory / 'file').write_text('data')
        (self.root / 'link').symlink_to(directory, target_is_directory=True)
        recipe = self.recipe()
        with self.assertRaises(ValueError):
            core.identity({**recipe, 'dependencies': ['link']})


class AuditTest(unittest.TestCase):
    def test_publication_status_exposes_missing_and_conflicted_receipts(self):
        from learning_audit.report import publication_status
        with tempfile.TemporaryDirectory() as temporary:
            receipt = Path(temporary) / 'latest.json'
            self.assertEqual(publication_status(receipt), {'state': 'missing'})
            value = {'lessons': 3, 'memory_projects': 2, 'skills': ['codex/a', 'claude/a'],
                     'conflicts': {'manual-skill': 'preserved'}}
            receipt.write_text(json.dumps(value))
            status = publication_status(receipt)
            self.assertEqual(status['state'], 'conflict')
            self.assertEqual(status['skill_installations'], 2)
            self.assertEqual(status['conflicts'], value['conflicts'])
            self.assertEqual(json.loads(receipt.read_text()), value)
            value['conflicts'] = {}
            receipt.write_text(json.dumps(value))
            self.assertEqual(publication_status(receipt)['state'], 'published')

    def test_sync_publishes_only_audit_source_and_preserves_project_scope(self):
        from learning_audit.report import sync_reports
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            (root / '.mike').mkdir()
            (root / '.mike/knowledge.json').write_text(json.dumps({'project_uuid': 'audit-project'}))
            output = root / 'artifacts/learning-audit'
            output.mkdir(parents=True)
            (output / 'report.md').write_text('# Audit\nObserved tool reuse')
            other = root / 'artifacts/learning-memory/other-project'
            other.mkdir(parents=True)
            (other / 'memory.md').write_text('# Private project instructions')
            with patch('learning_audit.report.ROOT', root), \
                 patch('knowledge.machine.machine_identity', return_value={'machine_uuid': 'machine'}), \
                 patch('knowledge.collector.Client') as client:
                client.return_value.request.side_effect = [
                    {'location_uuid': 'location'}, {'source_uuid': 'audit-source', 'generation': 3},
                    {'documents': 1}]
                self.assertEqual(sync_reports(output), {'documents': 1})
                calls = client.return_value.request.call_args_list
            self.assertEqual(calls[0].args[1]['project'], 'audit-project')
            self.assertEqual(calls[1].args[1]['root_path'], str(output))
            self.assertEqual(calls[1].args[1]['name'], 'weekly-learning-audit')
            self.assertEqual(calls[2].args[0], '/api/knowledge/sources/audit-source/scan')
            self.assertEqual(calls[2].args[1]['generation'], 3)
            self.assertEqual([d['relative_path'] for d in calls[2].args[1]['documents']], ['report.md'])

    def test_machine_only_activity_does_not_imply_missing_lesson_delivery(self):
        data = {'embedding_setting': [], 'injections': [], 'lessons': [],
                'effects': [{'family': 'action_effect', 'rows': 1}],
                'turns': [{'project': 'machine', 'turns': 20, 'genuine_turns': 0},
                          {'project': 'unknown', 'turns': 3, 'genuine_turns': 0,
                           'missing_provenance': 3}]}
        self.assertEqual([row['kind'] for row in findings(data)], ['outcome_provenance_incomplete'])

    def test_reports_disabled_and_missing_delivery_without_claiming_savings(self):
        data = {'embedding_setting': [{'value': False}], 'injections': [],
                'turns': [{'project': 'p', 'turns': 4}],
                'lessons': [{'project': 'p', 'source': 'operator', 'pending_embeddings': 2}], 'effects': []}
        self.assertEqual(len(findings(data)), 4)
        candidate = opportunity({'example_request_id': 42})
        self.assertEqual(candidate['status'], 'candidate_requires_validation')
        self.assertIn('not proven', candidate['savings_status'])

    def grammar_data(self, recall=(), update=(), harvested=()):
        return {'embedding_setting': [], 'injections': [], 'lessons': [], 'turns': [],
                'effects': [{'family': 'action_effect', 'rows': 1}],
                'memory_recall_tags': list(recall), 'memory_update_tags': list(update),
                'memory_harvested_lessons': list(harvested)}

    def llm_data(self, toggles=(), roles=(), days=None):
        data = self.grammar_data(update=[{'project': 'p', 'replies_with_tags': 1, 'tags_found': 1,
                                          'lessons_written': 1, 'lessons_skipped': 0, 'lessons_truncated': 0}])
        data.update({'worker_toggles': list(toggles), 'llm_roles': list(roles)})
        if days is not None:
            data['proposer_active_days'] = days
        return data

    def test_worker_toggle_off_for_days_is_a_finding(self):
        off = {'key': 'judge_lessons', 'enabled': False, 'days_in_state': '9.2'}
        fresh = {'key': 'judge_worker', 'enabled': False, 'days_in_state': '0.5'}
        on = {'key': 'steerer', 'enabled': True, 'days_in_state': '30.0'}
        with patch('learning_audit.report.skill_ledger', return_value={}):
            found = findings(self.llm_data(toggles=[off, fresh, on]))
        self.assertEqual([(f['kind'], f['evidence']['key']) for f in found],
                         [('worker_toggle_off', 'judge_lessons')])

    def test_llm_role_whose_last_call_failed_is_a_finding(self):
        failing = {'role': 'judge_worker', 'last_outcome': 'http_error', 'failed_streak': 5}
        healthy = {'role': 'steerer', 'last_outcome': 'ok', 'failed_streak': 0}
        with patch('learning_audit.report.skill_ledger', return_value={}):
            found = findings(self.llm_data(roles=[failing, healthy]))
        self.assertEqual([(f['kind'], f['evidence']['role']) for f in found],
                         [('llm_role_failing', 'judge_worker')])

    def test_proposer_active_days_render_per_month(self):
        from learning_audit.report import render_proposer_days
        days = [{'month': '2026-09', 'days_with_calls': 4}, {'month': '2026-10', 'days_with_calls': 2}]
        self.assertEqual(render_proposer_days(self.llm_data(days=days)),
                         ['- Days with at least one lesson proposer call, UTC month 2026-09: 4.',
                          '- Days with at least one lesson proposer call, UTC month 2026-10: 2.'])
        self.assertIn('No lesson proposer call', render_proposer_days(self.llm_data(days=[]))[0])
        self.assertEqual(render_proposer_days(self.llm_data()), [])

    def test_memory_grammar_unused_fires_when_no_tag_was_seen(self):
        with patch('learning_audit.report.skill_ledger', return_value={}):
            kinds = [row['kind'] for row in findings(self.grammar_data())]
        self.assertEqual(kinds, ['memory_grammar_unused'])

    def test_memory_grammar_quiet_when_update_tags_were_found(self):
        update = [{'project': 'p', 'replies_with_tags': 1, 'tags_found': 2, 'lessons_written': 1,
                   'lessons_skipped': 1, 'lessons_truncated': 0}]
        with patch('learning_audit.report.skill_ledger', return_value={}):
            self.assertEqual(findings(self.grammar_data(update=update)), [])

    def test_memory_grammar_quiet_when_recall_tags_were_seen(self):
        recall = [{'project': 'p', 'no_tag_requests': 9, 'injected': 1, 'delivered_out_of_band': 0,
                   'requests_with_recall_tags': 1}]
        with patch('learning_audit.report.skill_ledger', return_value={}):
            self.assertEqual(findings(self.grammar_data(recall=recall)), [])

    def test_memory_grammar_totals_and_rendering(self):
        from learning_audit.report import memory_grammar_totals, render_memory_grammar
        data = self.grammar_data(
            recall=[{'project': 'p', 'no_tag_requests': 5, 'injected': 2, 'delivered_out_of_band': 1,
                     'requests_with_recall_tags': 4}],
            update=[{'project': 'p', 'replies_with_tags': 2, 'tags_found': 3, 'lessons_written': 2,
                     'lessons_skipped': 1, 'lessons_truncated': 0}],
            harvested=[{'project': 'p', 'lessons': 2}])
        self.assertEqual(memory_grammar_totals(data), {
            'recall_tag_requests': 4, 'recall_injected': 3, 'update_tags_found': 3,
            'update_lessons_written': 2, 'update_lessons_skipped': 1, 'harvested_lessons': 2})
        text = '\n'.join(render_memory_grammar(data))
        self.assertIn('4 requests carried one; 3 led to lessons being delivered', text)
        self.assertIn('3 found in replies; 2 stored as lessons and 1 skipped', text)
        self.assertIn('| p | 4 | 3 | 3 | 2 | 1 |', text)

    def test_memory_grammar_section_absent_without_its_queries(self):
        from learning_audit.report import render_memory_grammar
        self.assertEqual(render_memory_grammar({}), [])

    def test_memory_grammar_queries_use_the_window_and_the_stage_markers(self):
        from learning_audit.queries import queries
        sql = queries('2026-10-07T00:00:00+00:00', '2026-10-14T00:00:00+00:00')
        self.assertIn('recall_lessons_injected', sql['memory_recall_tags'])
        self.assertIn('recall_no_tags', sql['memory_recall_tags'])
        self.assertIn('memory.harvest_runs', sql['memory_update_tags'])
        self.assertIn("src.code='harvester'", sql['memory_harvested_lessons'])
        for name in ('memory_recall_tags', 'memory_update_tags', 'memory_harvested_lessons'):
            self.assertIn('2026-10-14T00:00:00+00:00', sql[name])

    def test_live_skill_numbers_are_rendered_with_both_rates_and_flag_outcome_retirements(self):
        from learning_audit.report import live_findings, render_skill_live
        row = {'project': '/p/app', 'folder': 'deploy', 'first_shipped_on': '2026-10-14', 'loads': 12,
               'judged_turns': 8, 'corrected_turns': 4, 'base_judged_turns': 40, 'base_corrected_turns': 8,
               'retired_at': None, 'retired_reason': None}
        gone = {**row, 'folder': 'lint', 'loads': 0, 'judged_turns': 0, 'corrected_turns': 0,
                'retired_at': '2026-10-20T00:00:00+00:00', 'retired_reason': 'corrected_above_base'}
        text = '\n'.join(render_skill_live({'skill_live': [row, gone]}))
        self.assertIn('| /p/app | `deploy` | 2026-10-14 | 12 | 4 of 8 (50%) | 8 of 40 (20%) | shipped |', text)
        self.assertIn('| /p/app | `lint` | 2026-10-14 | 0 | no judged turn | 8 of 40 (20%) | retired (corrected_above_base) |', text)
        self.assertEqual([f['evidence']['folder'] for f in live_findings({'skill_live': [row, gone]})], ['lint'])
        self.assertEqual(live_findings({'skill_live': None}), [])
        self.assertIn('Not available', '\n'.join(render_skill_live({'skill_live': None})))
        self.assertIn('No shipped skill', '\n'.join(render_skill_live({'skill_live': []})))

    def test_audit_survives_a_proxy_that_has_not_applied_the_live_outcomes_migration(self):
        from learning_audit import report
        def fake(sql):
            if 'skill_live_outcomes' in sql:
                raise RuntimeError('relation "memory.skill_live_outcomes" does not exist')
            return []
        with patch.object(report, 'query', side_effect=fake):
            data = report.run_queries('2026-10-07T00:00:00+00:00', '2026-10-14T00:00:00+00:00')
        self.assertIsNone(data['skill_live'])
        self.assertEqual(data['tools'], [])
        with patch.object(report, 'query', side_effect=RuntimeError('database down')):
            with self.assertRaises(RuntimeError):
                report.run_queries('2026-10-07T00:00:00+00:00', '2026-10-14T00:00:00+00:00')

    def test_skill_loads_without_generated_skill_fire_finding(self):
        data = {'embedding_setting': [], 'injections': [], 'lessons': [], 'turns': [],
                'effects': [{'family': 'action_effect', 'rows': 1}],
                'skill_loads': [{'project': 'p', 'project_uuid': 'u1', 'skill': 'manual-skill', 'loads': 5, 'sessions': 2}]}
        with patch('learning_audit.report.skill_ledger', return_value={}):
            self.assertEqual(findings(data), [{'kind': 'generated_skills_never_loaded', 'project': 'global',
                                               'evidence': {'total_loads': 5, 'generated_loads': 0}}])

    def test_generated_skill_load_suppresses_skill_finding(self):
        data = {'embedding_setting': [], 'injections': [], 'lessons': [], 'turns': [],
                'effects': [{'family': 'action_effect', 'rows': 1}],
                'skill_loads': [{'project': 'p', 'project_uuid': 'u1', 'skill': 'manual-skill', 'loads': 5, 'sessions': 2},
                                {'project': 'p', 'project_uuid': 'u1', 'skill': 'mike-memory', 'loads': 1, 'sessions': 1}]}
        with patch('learning_audit.report.skill_ledger', return_value={}), \
             patch('learning_audit.report.skill_manifest_names', return_value={'mike-memory'}):
            self.assertNotIn('generated_skills_never_loaded', [row['kind'] for row in findings(data)])

    def test_load_counts_as_generated_through_the_manifest_whatever_the_name(self):
        data = {'embedding_setting': [], 'injections': [], 'lessons': [], 'turns': [],
                'effects': [{'family': 'action_effect', 'rows': 1}],
                'skill_loads': [{'project': 'p', 'project_uuid': 'u1', 'skill': 'lookalike', 'loads': 2, 'sessions': 1},
                                {'project': 'p', 'project_uuid': None, 'skill': 'rust-lint', 'loads': 1, 'sessions': 1}]}
        from learning_audit.report import generated_skill_loads
        with patch('learning_audit.report.skill_ledger', return_value={'u1:rust-lint': {}}), \
             patch('learning_audit.report.skill_manifest_names', return_value={'shared-workflows'}):
            self.assertEqual([r['generated'] for r in generated_skill_loads(data)], [False, True])
        with patch('learning_audit.report.skill_ledger', return_value={}), \
             patch('learning_audit.report.skill_manifest_names', return_value={'lookalike'}):
            self.assertEqual([r['generated'] for r in generated_skill_loads(data)], [True, False])

    def test_topic_skill_load_counts_as_generated_through_ledger(self):
        data = {'embedding_setting': [], 'injections': [], 'lessons': [], 'turns': [],
                'effects': [{'family': 'action_effect', 'rows': 1}],
                'skill_loads': [{'project': 'p', 'project_uuid': 'u1', 'skill': 'deploy-and-restart', 'loads': 3, 'sessions': 1}]}
        with patch('learning_audit.report.skill_ledger', return_value={'u1:deploy-and-restart': {}}):
            self.assertNotIn('generated_skills_never_loaded', [row['kind'] for row in findings(data)])


class PromotionTest(unittest.TestCase):
    def test_conflict_preserves_siblings_and_retirement_preserves_manual_edits(self):
        from learning_audit.promote import reconcile
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            state = root / 'state'
            paths = [root / client / leaf for client in ['codex', 'claude']
                     for leaf in ['SKILL.md', 'references/memory.md']]
            desired = {str(path): 'generated' for path in paths}
            manifest = {}
            self.assertFalse(reconcile(desired, manifest, state))
            paths[1].write_text('my edited memory')
            conflicts = reconcile(desired, manifest, state)
            self.assertIn(str(paths[1]), conflicts)
            self.assertTrue(all(path.exists() for path in paths))
            retired = reconcile({}, manifest, state)
            self.assertIn(str(paths[1]), retired)
            self.assertEqual(paths[1].read_text(), 'my edited memory')
            self.assertTrue(all(not path.exists() for path in paths if path != paths[1]))
            self.assertEqual(len(list((state / 'retired').glob('*.md'))), 3)

    PROJECT_UUID = 'abcdef12-0000-4000-8000-000000000000'
    OTHER_UUID = '11111111-0000-4000-8000-000000000000'

    def lesson(self, id, topic, kind='correction', text=None, path=None, agent=None, uuid=None, placement='skill'):
        return {'id': id, 'text': text or f'Lesson {id} text.', 'topic': topic, 'source_turn_id': 900 + id,
                'stage': 'long_term', 'source': 'operator', 'kind': kind,
                'project_uuid': uuid or self.PROJECT_UUID, 'project_path': path,
                'agent_id': None if agent is None else 7, 'agent_name': agent, 'placement': placement}

    def promote_once(self, home, lessons, loads=(), drafts=(), locations=(), cap=None, modes=None, retired=(),
                     live=(), recipes=()):
        modes = {self.PROJECT_UUID: 'auto', self.OTHER_UUID: 'auto'} if modes is None else modes
        from learning_audit import promote as promote_module
        project = home / 'work' / 'proj'
        project.mkdir(parents=True, exist_ok=True)
        (project / '.git').mkdir(exist_ok=True)
        for row in lessons:
            if row['project_path'] is None:
                row['project_path'] = str(project)
        writes = []
        real_write = promote_module.atomic_write
        def record(path, body):
            writes.append(Path(path))
            real_write(path, body)
        def fake_query(sql):
            if 'WITH shipped' in sql:
                return list(live)
            if 'tools.uses' in sql:
                if isinstance(loads, dict):
                    return [{'project_uuid': self.PROJECT_UUID, 'skill': skill, 'loads': count} for skill, count in loads.items()]
                return [{'project_uuid': self.PROJECT_UUID, 'skill': skill} for skill in loads]
            if 'config.settings' in sql:
                return [] if cap is None else [{'value': cap}]
            if 'projects.locations' in sql:
                return list(locations)
            if 'skill_drafts' in sql:
                return list(drafts)
            if 'skill_policies' in sql:
                return [{'project_uuid': uuid, 'mode': mode} for uuid, mode in modes.items()]
            if 'skill_retirements' in sql:
                return [{'project_uuid': self.PROJECT_UUID, 'folder': folder} for folder in retired]
            return lessons
        # Temporary checkouts sit under /tmp on Linux, which the real guard refuses, so only paths under this temp home count as checkouts here; the guard is tested on its own below.
        with patch.object(Path, 'home', return_value=home), \
             patch.object(promote_module, 'ROOT', home / 'repo'), \
             patch.object(promote_module, 'query', side_effect=fake_query), \
             patch.object(promote_module, 'catalog', return_value=list(recipes)), \
             patch.object(promote_module, 'eligible_checkout', side_effect=lambda path: str(path).startswith(str(home))), \
             patch.object(promote_module, 'atomic_write', side_effect=record):
            result = promote_module.promote(sync=False, skill_roots=[home / 'skills'])
        self.assertTrue(writes)
        self.assertTrue(all(home in path.parents for path in writes), writes)
        return result

    def test_recipe_skill_lands_in_its_checkout_never_user_wide(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            project = home / 'work' / 'proj'
            recipes = [{'id': 'outline', 'description': 'Index headings', 'cwd': str(project), 'contract': 'read-only'},
                       {'id': 'gone', 'description': 'd', 'cwd': '/nonexistent/checkout', 'contract': 'k'}]
            result = self.promote_once(home, [], recipes=recipes)
            self.assertTrue((project / '.claude/skills/tool-outline/SKILL.md').exists())
            self.assertTrue((project / '.codex/skills/tool-outline/SKILL.md').exists())
            self.assertTrue(all(path.startswith(str(project)) for path in result['skills']), result['skills'])
            self.assertFalse((home / 'skills').exists())
            self.assertIn('.claude/skills/tool-outline/', (project / '.git/info/exclude').read_text())

    def test_retired_file_takes_its_emptied_skill_directory_with_it(self):
        from learning_audit.promote import reconcile
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            skill = root / 'skills' / 'tool-old' / 'SKILL.md'
            skill.parent.mkdir(parents=True)
            skill.write_text('generated')
            manifest = {str(skill): hashlib.sha256(b'generated').hexdigest()}
            reconcile({}, manifest, root / 'state')
            self.assertFalse(skill.parent.exists())

    def test_topic_skill_lands_in_project_checkout_and_names_the_work(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy-and-restart', text='Restart the proxy with start-proxy.sh.'),
                       self.lesson(2, 'deploy-and-restart', kind='fact', text='Deploy only from the main checkout.'),
                       self.lesson(3, 'lint'), self.lesson(4, 'verifier:met'), self.lesson(5, 'verifier:met'),
                       self.lesson(6, None), self.lesson(7, None),
                       self.lesson(8, 'deploy-and-restart', path='/Users/dawid/absent-checkout', uuid=self.OTHER_UUID),
                       self.lesson(9, 'deploy-and-restart', path='/Users/dawid/absent-checkout', uuid=self.OTHER_UUID)]
            result = self.promote_once(home, lessons)
            project = home / 'work' / 'proj'
            skill_dir = project / '.claude/skills/deploy-and-restart'
            body = (skill_dir / 'SKILL.md').read_text()
            self.assertIn('name: deploy-and-restart\n', body)
            self.assertIn('description: "Deploy and restart: 2 accepted lessons for the `proj` project, kinds correction, fact. Use when running or touching `start-proxy.sh`, or when the task involves deploy and restart."', body)
            self.assertIn('The lessons are in [references/lessons.md](references/lessons.md).', body)
            self.assertNotIn('## Lesson', body)
            reference = (skill_dir / 'references/lessons.md').read_text()
            self.assertIn('Lessons behind the deploy-and-restart skill for the `proj` project, each with its source turn.', reference)
            self.assertIn('Restart the proxy with start-proxy.sh.', reference)
            self.assertIn('Deploy only from the main checkout.', reference)
            self.assertFalse((skill_dir / 'evals').exists())
            self.assertEqual(result['authored_skills'], 0)
            self.assertEqual(sorted(p.name for p in (project / '.claude/skills').iterdir()), ['deploy-and-restart'])
            self.assertFalse((home / 'skills').exists())
            self.assertEqual((project / '.git/info/exclude').read_text(), '.claude/skills/deploy-and-restart/\n')
            self.assertEqual((result['topic_skills'], result['topic_skill_projects'], result['skipped_projects']),
                             (1, 1, ['/Users/dawid/absent-checkout']))

    def test_fallback_description_names_commands_and_files_from_the_lessons(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'rust-lint', text='Never accept `cargo clippy -p` as the lint gate; run scripts/gate lint.'),
                       self.lesson(2, 'rust-lint', text='Use `cargo clippy -p` only for a quick look, then `make lint`.')]
            self.promote_once(home, lessons)
            body = (home / 'work/proj/.claude/skills/rust-lint/SKILL.md').read_text()
            self.assertIn('Use when running or touching `cargo clippy -p`, `scripts/gate`, `make lint`, or when the task involves rust lint.', body)

    def test_triggers_ignore_urls_plain_words_and_absolute_paths(self):
        from learning_audit.promote import triggers
        rows = [{'text': 'See `main` and `https://x.y/z` and /abs/path/file.rs, also `make test` and tests/run.sh.'}]
        self.assertEqual(triggers(rows), ['make test', 'tests/run.sh'])
        self.assertEqual(triggers([{'text': 'nothing here'}]), [])

    def test_checkout_found_through_a_location_registered_by_this_machine(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy', path='/mnt/data/elsewhere/proj'), self.lesson(2, 'deploy', path='/mnt/data/elsewhere/proj')]
            local = home / 'work' / 'proj'
            result = self.promote_once(home, lessons, locations=[{'project_uuid': self.PROJECT_UUID, 'root_path': str(local)}])
            self.assertEqual((result['topic_skills'], result['skipped_projects']), (1, []))
            self.assertTrue((local / '.claude/skills/deploy/SKILL.md').exists())

    def test_agent_scoped_topic_skill_names_the_agent(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy-and-restart', agent='rust-developer'),
                       self.lesson(2, 'deploy-and-restart', kind='fact', agent='rust-developer')]
            self.promote_once(home, lessons)
            skills = home / 'work/proj/.claude/skills'
            self.assertEqual(sorted(p.name for p in skills.iterdir()), ['deploy-and-restart-rust-developer'])
            body = (skills / 'deploy-and-restart-rust-developer/SKILL.md').read_text()
            self.assertIn('name: deploy-and-restart-rust-developer\n', body)
            self.assertIn('Applies to the rust-developer agent only."', body)

    def test_exclude_gets_one_line_per_topic_skill_and_tracked_skills_stay_untouched(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            project = home / 'work' / 'proj'
            project.mkdir(parents=True)
            subprocess.run(['git', 'init', '-q', str(project)], check=True)
            hand = project / '.claude/skills/hand-written/SKILL.md'
            hand.parent.mkdir(parents=True)
            hand.write_text('hand-written')
            subprocess.run(['git', '-C', str(project), 'add', '.claude/skills/hand-written/SKILL.md'], check=True)
            lessons = [self.lesson(1, 'deploy-and-restart'), self.lesson(2, 'deploy-and-restart'),
                       self.lesson(3, 'lint'), self.lesson(4, 'lint')]
            self.promote_once(home, lessons)
            self.promote_once(home, lessons)
            exclude = (project / '.git/info/exclude').read_text().splitlines()
            self.assertEqual(exclude.count('.claude/skills/deploy-and-restart/'), 1)
            self.assertEqual(exclude.count('.claude/skills/lint/'), 1)
            self.assertNotIn('.claude/skills/', exclude)
            self.assertEqual(hand.read_text(), 'hand-written')
            self.assertEqual(subprocess.run(['git', '-C', str(project), 'check-ignore', '-q',
                                             '.claude/skills/deploy-and-restart/SKILL.md']).returncode, 0)
            status = subprocess.run(['git', '-C', str(project), 'status', '--porcelain'],
                                    capture_output=True, text=True, check=True).stdout.splitlines()
            self.assertEqual(status, ['A  .claude/skills/hand-written/SKILL.md'])

    def test_eligible_checkout_refuses_scratch_state_and_relative_paths(self):
        from learning_audit.promote import eligible_checkout
        repo = Path(__file__).resolve().parents[2]
        with patch.object(Path, 'home', return_value=Path('/home/example')):
            for refused in ['x', '.claude', '/home/example/.local/state/mike/seats/a/c4',
                            '/srv/clones/tmp/c4-plain-s3', '/var/tmp/clone', '/nonexistent/repo']:
                self.assertFalse(eligible_checkout(refused), refused)
            self.assertTrue(eligible_checkout(repo))

    def test_manifest_retires_global_topic_and_generic_skills(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            state = home / '.local/state/mike-learning-promotion'
            state.mkdir(parents=True)
            old = {home / 'skills/proj-abcdef12-deploy-and-restart/SKILL.md': 'old topic',
                   home / 'skills/proj-abcdef12/SKILL.md': 'old generic',
                   home / 'skills/proj-abcdef12/references/memory.md': 'old memory'}
            manifest = {}
            for path, body in old.items():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(body)
                manifest[str(path)] = hashlib.sha256(body.encode()).hexdigest()
            (state / 'manifest.json').write_text(json.dumps(manifest))
            self.promote_once(home, [self.lesson(1, 'deploy-and-restart'), self.lesson(2, 'deploy-and-restart')])
            self.assertTrue(all(not path.exists() for path in old))
            kept = json.loads((state / 'manifest.json').read_text())
            self.assertFalse({str(path) for path in old} & set(kept))
            self.assertEqual(len(list((state / 'retired').glob('*.md'))), 3)
            self.assertTrue((home / 'work/proj/.claude/skills/deploy-and-restart/SKILL.md').exists())

    def test_retire_unloaded_drops_only_stale_unloaded_topic_skills(self):
        from learning_audit.promote import retire_unloaded
        today = date(2026, 10, 14)
        stale = (today - timedelta(days=31)).isoformat()
        fresh = (today - timedelta(days=10)).isoformat()
        def entry(first):
            return {'first_emitted': first, 'retired_at': None, 'lesson_watermark': 2}
        def key(folder):
            return f'{self.PROJECT_UUID}:{folder}'
        emitted = {key('stale'): Path('/p/.claude/skills/stale/SKILL.md'),
                   key('fresh'): Path('/p/.claude/skills/fresh/SKILL.md'),
                   key('loaded'): Path('/p/.claude/skills/loaded/SKILL.md')}
        desired = {str(path): 'x' for path in emitted.values()}
        desired['/h/skills/proj-abcdef12/SKILL.md'] = 'generic'
        desired['/h/skills/shared-workflows/SKILL.md'] = 'shared'
        ledger = {key('stale'): entry(stale), key('fresh'): entry(fresh),
                  key('loaded'): entry(stale), key('vanished'): entry(stale)}
        retired = retire_unloaded(desired, ledger, {key('loaded')}, today, emitted)
        self.assertEqual(retired, [key('stale')])
        self.assertEqual(set(desired), {'/h/skills/proj-abcdef12/SKILL.md', '/h/skills/shared-workflows/SKILL.md',
                                        '/p/.claude/skills/fresh/SKILL.md', '/p/.claude/skills/loaded/SKILL.md'})
        self.assertEqual(ledger[key('stale')]['retired_at'], today.isoformat())
        self.assertIsNone(ledger[key('fresh')]['retired_at'])
        self.assertIsNone(ledger[key('vanished')]['retired_at'])

    def test_retired_topic_skill_stays_retired_until_it_gains_a_lesson(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            ledger = home / '.local/state/mike-learning-promotion/skill-installs.json'
            key = f'{self.PROJECT_UUID}:deploy-and-restart'
            skill = home / 'work/proj/.claude/skills/deploy-and-restart/SKILL.md'
            lessons = [self.lesson(1, 'deploy-and-restart'), self.lesson(2, 'deploy-and-restart')]
            self.assertEqual(self.promote_once(home, lessons)['topic_skills'], 1)
            self.assertTrue(skill.exists())
            entries = json.loads(ledger.read_text())
            entries[key]['first_emitted'] = (date.today() - timedelta(days=31)).isoformat()
            ledger.write_text(json.dumps(entries))
            self.assertEqual(self.promote_once(home, lessons)['retired'], [key])
            self.assertFalse(skill.exists())
            self.assertFalse((skill.parent / 'references/lessons.md').exists())
            self.assertIsNotNone(json.loads(ledger.read_text())[key]['retired_at'])
            self.assertEqual(self.promote_once(home, lessons)['held_retired'], [key])
            self.assertFalse(skill.exists())
            result = self.promote_once(home, lessons + [self.lesson(3, 'deploy-and-restart')])
            self.assertEqual(result['held_retired'], [])
            self.assertTrue(skill.exists())
            self.assertIsNone(json.loads(ledger.read_text())[key]['retired_at'])

    def live_row(self, folder, loads, judged, corrected, base_judged=40, base_corrected=8):
        return {'project_uuid': self.PROJECT_UUID, 'folder': folder,
                'window_start': '2026-10-14T00:00:00+00:00', 'window_end': '2026-10-15T00:00:00+00:00',
                'loads': loads, 'load_sessions': 3, 'loaded_turns': judged + 1, 'judged_turns': judged,
                'corrected_turns': corrected, 'base_judged_turns': base_judged, 'base_corrected_turns': base_corrected}

    def test_skill_with_ten_loads_and_a_corrected_rate_above_base_is_retired(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            ledger = home / '.local/state/mike-learning-promotion/skill-installs.json'
            key = f'{self.PROJECT_UUID}:deploy-and-restart'
            skill = home / 'work/proj/.claude/skills/deploy-and-restart/SKILL.md'
            lessons = [self.lesson(1, 'deploy-and-restart'), self.lesson(2, 'deploy-and-restart')]
            self.promote_once(home, lessons, loads=['deploy-and-restart'])
            self.assertTrue(skill.exists())
            result = self.promote_once(home, lessons, loads=['deploy-and-restart'],
                                       live=[self.live_row('deploy-and-restart', loads=12, judged=8, corrected=4)])
            self.assertEqual(result['retired_corrected_above_base'], [key])
            self.assertEqual(result['retired'], [])
            self.assertFalse(skill.exists())
            self.assertFalse((skill.parent / 'references/lessons.md').exists())
            entry = json.loads(ledger.read_text())[key]
            self.assertEqual(entry['retired_reason'], 'corrected_above_base')
            self.assertEqual(entry['outcome']['retired_reason'], 'corrected_above_base')
            self.assertEqual((entry['outcome']['loads'], entry['outcome']['corrected_turns']), (12, 4))
            self.assertEqual(entry['outcome']['first_shipped_on'], date.today().isoformat())
            self.assertIn('T', entry['outcome']['retired_at'])
            self.assertFalse(entry['published'])
            self.assertEqual(self.promote_once(home, lessons)['held_retired'], [key])

    def test_skill_with_few_loads_or_a_rate_at_or_below_base_is_kept(self):
        from learning_audit.live import corrected_above_base
        def row(loads, judged, corrected, base_judged=40, base_corrected=8):
            return self.live_row('x', loads, judged, corrected, base_judged, base_corrected)
        self.assertTrue(corrected_above_base(row(10, 8, 4)))
        self.assertFalse(corrected_above_base(row(9, 8, 8)))
        self.assertFalse(corrected_above_base(row(30, 10, 2)))
        self.assertFalse(corrected_above_base(row(30, 10, 1)))
        self.assertFalse(corrected_above_base(row(30, 0, 0)))
        self.assertFalse(corrected_above_base(row(30, 5, 5, base_judged=0, base_corrected=0)))
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy-and-restart'), self.lesson(2, 'deploy-and-restart')]
            self.promote_once(home, lessons, loads=['deploy-and-restart'])
            result = self.promote_once(home, lessons, loads=['deploy-and-restart'],
                                       live=[self.live_row('deploy-and-restart', loads=12, judged=8, corrected=1)])
            self.assertEqual(result['retired_corrected_above_base'], [])
            self.assertEqual(result['live_skills'], 1)
            self.assertTrue((home / 'work/proj/.claude/skills/deploy-and-restart/SKILL.md').exists())

    def test_measure_asks_only_for_skills_the_ledger_owns_and_names_them_safely(self):
        from learning_audit.live import measure
        good = f'{self.PROJECT_UUID}:deploy-and-restart'
        ledger = {good: {'first_emitted': '2026-10-01'},
                  f"{self.PROJECT_UUID}:x'); DROP TABLE y; --": {'first_emitted': '2026-10-01'},
                  'not-a-uuid:fine': {'first_emitted': '2026-10-01'},
                  f'{self.OTHER_UUID}:late': {'first_emitted': 'yesterday'}}
        asked = []
        def fake(sql):
            asked.append(sql)
            return [self.live_row('deploy-and-restart', 3, 2, 0)]
        found = measure(fake, ledger, list(ledger))
        self.assertEqual(list(found), [good])
        self.assertEqual(found[good]['first_shipped_on'], '2026-10-01')
        self.assertEqual(len(asked), 1)
        self.assertIn(f"('{self.PROJECT_UUID}'::uuid,'deploy-and-restart','2026-10-01'::date)", asked[0])
        self.assertNotIn('DROP', asked[0])
        self.assertNotIn('not-a-uuid', asked[0])
        self.assertEqual(measure(fake, {}, []), {})
        self.assertEqual(len(asked), 1)

    def test_publish_sends_unpublished_outcomes_and_retries_after_a_failure(self):
        from learning_audit.live import publish
        sent = []
        class Proxy:
            def __init__(self, fail):
                self.fail = fail
            def request(self, path, data=None, method=None):
                if self.fail:
                    raise RuntimeError('Mike returned 404: not found')
                sent.append((path, data))
                return {'stored': len(data['outcomes'])}
        ledger = {'a:1': {'outcome': {'folder': 'one'}, 'published': False},
                  'a:2': {'outcome': {'folder': 'two'}, 'published': True},
                  'a:3': {'retired_at': '2026-10-01'}}
        failed = publish(Proxy(True), ledger)
        self.assertEqual(failed['sent'], 0)
        self.assertIn('404', failed['error'])
        self.assertFalse(ledger['a:1']['published'])
        done = publish(Proxy(False), ledger)
        self.assertEqual(done, {'sent': 1, 'error': None})
        self.assertEqual(sent, [('/api/learning/skill-outcomes', {'outcomes': [{'folder': 'one'}]})])
        self.assertTrue(ledger['a:1']['published'])
        self.assertEqual(publish(Proxy(False), ledger)['sent'], 0)
        self.assertEqual(len(sent), 1)

    def test_accepted_draft_authors_the_skill_and_its_references(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            draft = {'project_uuid': self.PROJECT_UUID, 'folder': 'deploy-and-restart', 'lesson_watermark': 2,
                     'skill_md': '---\nname: deploy-and-restart\ndescription: "Drafted."\n---\n\nDrafted body.\n',
                     'reference_files': '[{"name": "restarting", "when_to_read": "you restart.", "content": "# Restarting\\n"},'
                                        ' {"name": "lessons", "when_to_read": "x", "content": "overwrite"},'
                                        ' {"name": "../escape", "when_to_read": "x", "content": "escape"}]'}
            result = self.promote_once(home, [self.lesson(1, 'deploy-and-restart'), self.lesson(2, 'deploy-and-restart')], drafts=[draft])
            skill_dir = home / 'work/proj/.claude/skills/deploy-and-restart'
            self.assertEqual((skill_dir / 'SKILL.md').read_text(), draft['skill_md'])
            self.assertEqual((skill_dir / 'references/restarting.md').read_text(), '# Restarting\n')
            self.assertNotEqual((skill_dir / 'references/lessons.md').read_text(), 'overwrite')
            self.assertFalse((skill_dir / 'escape.md').exists())
            self.assertFalse((skill_dir / 'evals').exists())
            self.assertTrue((skill_dir / 'references/lessons.md').exists())
            self.assertEqual(result['authored_skills'], 1)

    def test_draft_older_than_the_newest_lesson_keeps_shipping_with_every_lesson_in_its_reference(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            draft = {'project_uuid': self.PROJECT_UUID, 'folder': 'deploy-and-restart', 'lesson_watermark': 1,
                     'skill_md': 'older draft'}
            result = self.promote_once(home, [self.lesson(1, 'deploy-and-restart'), self.lesson(2, 'deploy-and-restart')], drafts=[draft])
            skill_dir = home / 'work/proj/.claude/skills/deploy-and-restart'
            self.assertEqual((skill_dir / 'SKILL.md').read_text(), 'older draft')
            self.assertEqual((skill_dir / 'references/lessons.md').read_text().count('## Lesson '), 2)
            self.assertEqual(result['authored_skills'], 1)

    def skill_names(self, home):
        skills = home / 'work/proj/.claude/skills'
        return sorted(p.name for p in skills.iterdir()) if skills.exists() else []

    def test_only_lessons_placed_as_skills_feed_a_skill(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy', placement='memory'), self.lesson(2, 'deploy', placement='memory'),
                       self.lesson(3, 'lint', placement='rule'), self.lesson(4, 'lint', placement='rule'),
                       self.lesson(5, 'review', placement='agent'), self.lesson(6, 'review', placement='agent'),
                       self.lesson(7, 'gate', placement='nowhere'), self.lesson(8, 'gate', placement='nowhere'),
                       self.lesson(9, 'fleet', placement=None), self.lesson(10, 'fleet', placement=None),
                       self.lesson(11, 'testing'), self.lesson(12, 'testing')]
            result = self.promote_once(home, lessons)
            self.assertEqual(self.skill_names(home), ['fleet', 'testing'])
            self.assertEqual(result['topic_skills'], 2)

    def test_a_reviewed_skill_stays_when_its_lessons_are_placed_elsewhere(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            draft = {'project_uuid': self.PROJECT_UUID, 'folder': 'deploy-and-restart', 'lesson_watermark': 2,
                     'skill_md': 'reviewed draft', 'lesson_ids': [1, 2]}
            lessons = [self.lesson(1, 'deploy-and-restart', placement='memory'),
                       self.lesson(2, 'deploy-and-restart', placement='rule'),
                       self.lesson(3, 'unreviewed', placement='memory'), self.lesson(4, 'unreviewed', placement='memory')]
            self.promote_once(home, lessons, drafts=[draft])
            self.assertEqual(self.skill_names(home), ['deploy-and-restart'])
            skill_dir = home / 'work/proj/.claude/skills/deploy-and-restart'
            self.assertEqual((skill_dir / 'SKILL.md').read_text(), 'reviewed draft')
            self.assertEqual((skill_dir / 'references/lessons.md').read_text().count('## Lesson '), 2)

    def test_lessons_covered_by_their_own_skill_keep_feeding_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            covered = [dict(self.lesson(i, 'gate', placement='nowhere'), covered_by='gate/SKILL.md') for i in (1, 2)]
            elsewhere = [dict(self.lesson(i, 'lint', placement='nowhere'), covered_by='gate/SKILL.md') for i in (3, 4)]
            noted = [dict(self.lesson(i, 'docs', placement='nowhere'), covered_by='docs-style.md') for i in (5, 6)]
            self.promote_once(home, covered + elsewhere + noted)
            self.assertEqual(self.skill_names(home), ['gate'])

    def test_unplaced_lessons_keep_their_skill_until_placed(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy', placement=None), self.lesson(2, 'deploy', placement=None)]
            self.promote_once(home, lessons)
            self.assertEqual(self.skill_names(home), ['deploy'])

    def test_a_topic_with_one_skill_lesson_among_memory_lessons_is_not_enough(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy'), self.lesson(2, 'deploy', placement='memory'),
                       self.lesson(3, 'deploy', placement='memory')]
            self.promote_once(home, lessons)
            self.assertEqual(self.skill_names(home), [])

    def test_agent_scoped_lessons_not_placed_as_skills_make_no_agent_skill(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy', agent='rust-developer', placement='memory'),
                       self.lesson(2, 'deploy', agent='rust-developer', placement='memory')]
            self.promote_once(home, lessons)
            self.assertEqual(self.skill_names(home), [])

    def test_agents_sharing_a_topic_share_one_skill_without_an_agent_suffix(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            other = self.lesson(2, 'deploy', agent='db-dev')
            other['agent_id'] = 8
            self.promote_once(home, [self.lesson(1, 'deploy', agent='rust-developer'), other])
            self.assertEqual(self.skill_names(home), ['deploy'])

    def test_two_topics_with_the_same_purpose_ship_one_skill(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploying-the-proxy'), self.lesson(2, 'deploying-the-proxy'),
                       self.lesson(3, 'proxy-deploy-steps'), self.lesson(4, 'proxy-deploy-steps'),
                       self.lesson(5, 'proxy-deploy-steps')]
            result = self.promote_once(home, lessons)
            self.assertEqual(self.skill_names(home), ['proxy-deploy-steps'])
            self.assertEqual(result['purpose_held'], {f'{self.PROJECT_UUID}:deploying-the-proxy': f'{self.PROJECT_UUID}:proxy-deploy-steps'})

    def old_ledger(self, home, folders, days):
        ledger = home / '.local/state/mike-learning-promotion/skill-installs.json'
        ledger.parent.mkdir(parents=True, exist_ok=True)
        first = (date.today() - timedelta(days=days)).isoformat()
        ledger.write_text(json.dumps({f'{self.PROJECT_UUID}:{folder}': {'first_emitted': first, 'retired_at': None, 'lesson_watermark': 2}
                                      for folder in folders}))
        return ledger

    def test_a_new_skill_over_the_cap_retires_the_skill_with_the_least_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            ledger = self.old_ledger(home, ['alpha', 'bravo'], 20)
            lessons = [self.lesson(i, topic) for topic in ['alpha', 'bravo', 'charlie'] for i in range({'alpha': 1, 'bravo': 3, 'charlie': 5}[topic], {'alpha': 3, 'bravo': 5, 'charlie': 7}[topic])]
            result = self.promote_once(home, lessons, loads={'alpha': 9, 'bravo': 1}, cap=2)
            self.assertEqual(result['capped_retired'], [f'{self.PROJECT_UUID}:bravo'])
            self.assertEqual(result['held_for_cap'], [])
            self.assertEqual(self.skill_names(home), ['alpha', 'charlie'])
            self.assertIsNotNone(json.loads(ledger.read_text())[f'{self.PROJECT_UUID}:bravo']['retired_at'])

    def test_a_new_skill_is_held_when_every_existing_skill_is_younger_than_a_week(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            self.old_ledger(home, ['alpha', 'bravo'], 3)
            lessons = [self.lesson(i, topic) for topic in ['alpha', 'bravo', 'charlie'] for i in range({'alpha': 1, 'bravo': 3, 'charlie': 5}[topic], {'alpha': 3, 'bravo': 5, 'charlie': 7}[topic])]
            result = self.promote_once(home, lessons, cap=2)
            self.assertEqual(result['capped_retired'], [])
            self.assertEqual(result['held_for_cap'], [f'{self.PROJECT_UUID}:charlie'])
            self.assertEqual(self.skill_names(home), ['alpha', 'bravo'])

    def test_the_cap_defaults_to_twelve_and_ignores_a_bad_setting(self):
        from learning_audit.skill_budget import DEFAULT_CAP, read_cap
        self.assertEqual(DEFAULT_CAP, 12)
        self.assertEqual(read_cap(lambda sql: []), 12)
        self.assertEqual(read_cap(lambda sql: [{'value': 5}]), 5)
        for bad in [0, 101, 'many', True, None]:
            self.assertEqual(read_cap(lambda sql, bad=bad: [{'value': bad}]), 12)

    def draft(self, id, review=None, watermark=2, body='Drafted body.'):
        return {'id': id, 'project_uuid': self.PROJECT_UUID, 'folder': 'deploy-and-restart',
                'lesson_watermark': watermark, 'revision': 1, 'review': review,
                'skill_md': f'---\nname: deploy-and-restart\ndescription: "D."\n---\n\n{body}\n'}

    def test_ask_project_without_a_policy_row_writes_nothing_until_approved(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy-and-restart'), self.lesson(2, 'deploy-and-restart')]
            skill = home / 'work/proj/.claude/skills/deploy-and-restart/SKILL.md'
            result = self.promote_once(home, lessons, drafts=[self.draft(5)], modes={})
            self.assertFalse(skill.exists())
            self.assertEqual(result['topic_skills'], 0)
            self.assertEqual(result['held_for_approval'], [f'{self.PROJECT_UUID}:deploy-and-restart'])
            result = self.promote_once(home, lessons, drafts=[self.draft(5, review='approved', body='Approved.')], modes={})
            self.assertIn('Approved.', skill.read_text())
            self.assertEqual(result['authored_skills'], 1)
            ledger = json.loads((home / '.local/state/mike-learning-promotion/skill-installs.json').read_text())
            entry = ledger[f'{self.PROJECT_UUID}:deploy-and-restart']
            self.assertEqual((entry['draft_id'], entry['path']), (5, str(skill)))

    def test_switching_to_ask_removes_a_skill_written_without_approval(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy-and-restart'), self.lesson(2, 'deploy-and-restart')]
            skill = home / 'work/proj/.claude/skills/deploy-and-restart/SKILL.md'
            self.promote_once(home, lessons)
            self.assertTrue(skill.exists())
            self.promote_once(home, lessons, modes={self.PROJECT_UUID: 'ask'})
            self.assertFalse(skill.exists())
            ledger = json.loads((home / '.local/state/mike-learning-promotion/skill-installs.json').read_text())
            self.assertEqual(ledger[f'{self.PROJECT_UUID}:deploy-and-restart']['retired_reason'], 'policy')
            self.promote_once(home, lessons)
            self.assertTrue(skill.exists())

    def test_user_retired_folder_is_removed_and_not_written_until_restored(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy-and-restart'), self.lesson(2, 'deploy-and-restart')]
            skill = home / 'work/proj/.claude/skills/deploy-and-restart/SKILL.md'
            approved = [self.draft(5, review='approved')]
            self.promote_once(home, lessons, drafts=approved)
            self.assertTrue(skill.exists())
            result = self.promote_once(home, lessons + [self.lesson(3, 'deploy-and-restart')], drafts=approved,
                                       retired=['deploy-and-restart'])
            self.assertFalse(skill.exists())
            self.assertEqual(result['held_by_user'], [f'{self.PROJECT_UUID}:deploy-and-restart'])
            self.promote_once(home, lessons, drafts=approved)
            self.assertTrue(skill.exists())

    def test_auto_project_ships_the_newest_reviewed_draft(self):
        from learning_audit.promote import chosen_draft
        accepted, approved = self.draft(5, watermark=2), self.draft(6, review='approved', watermark=1)
        self.assertEqual(chosen_draft('auto', 2, accepted, approved)['id'], 5)
        self.assertEqual(chosen_draft('auto', 2, self.draft(5, watermark=1), self.draft(6, review='approved', watermark=2))['id'], 6)
        self.assertIsNone(chosen_draft('ask', 2, accepted, None))

    def test_auto_project_rejection_does_not_bring_back_an_earlier_revision(self):
        from learning_audit.promote import chosen_draft
        older, newest = self.draft(5, watermark=1), self.draft(6, review='rejected', watermark=2)
        self.assertIsNone(chosen_draft('auto', 2, older, None, newest))
        later = self.draft(7, watermark=3)
        self.assertEqual(chosen_draft('auto', 3, later, None, newest)['id'], 7)
        approved = self.draft(4, review='approved', watermark=1)
        self.assertEqual(chosen_draft('auto', 2, older, approved, newest)['id'], 4)

    def test_auto_project_writes_nothing_for_a_folder_whose_newest_draft_the_user_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            lessons = [self.lesson(1, 'deploy-and-restart'), self.lesson(2, 'deploy-and-restart')]
            skill = home / 'work/proj/.claude/skills/deploy-and-restart/SKILL.md'
            self.promote_once(home, lessons, drafts=[self.draft(5, body='Shipped.')])
            self.assertIn('Shipped.', skill.read_text())
            result = self.promote_once(home, lessons, drafts=[self.draft(5, review='rejected', body='Shipped.')])
            self.assertFalse(skill.exists())
            self.assertEqual(result['held_by_user'], [f'{self.PROJECT_UUID}:deploy-and-restart'])
            ledger = json.loads((home / '.local/state/mike-learning-promotion/skill-installs.json').read_text())
            self.assertEqual(ledger[f'{self.PROJECT_UUID}:deploy-and-restart']['retired_reason'], 'user')

    def test_an_accepted_draft_keeps_shipping_after_its_topic_gains_a_lesson(self):
        from learning_audit.promote import chosen_draft
        self.assertEqual(chosen_draft('auto', 3, self.draft(5, watermark=2), None)['id'], 5)

    def test_install_report_carries_retired_entries_and_survives_an_unreachable_proxy(self):
        from learning_audit.promote import report_installs
        sent = []
        class Recorder:
            def request(self, path, data):
                sent.append((path, data))
                return {'recorded': len(data['installs'])}
        ledger = {f'{self.PROJECT_UUID}:a': {'path': '/p/a/SKILL.md', 'draft_id': 3, 'lesson_watermark': 9,
                                              'first_emitted': '2026-10-01', 'retired_at': '2026-10-10', 'retired_reason': 'user'},
                  f'{self.PROJECT_UUID}:old': {'lesson_watermark': 1, 'first_emitted': '2026-09-01', 'retired_at': None}}
        machine = {'machine_uuid': 'm', 'machine_name': 'mac'}
        self.assertEqual(report_installs(Recorder(), machine, ledger), {'recorded': 1})
        install = sent[0][1]['installs'][0]
        self.assertEqual((sent[0][0], install['folder'], install['retired_reason']), ('/api/skills/installs', 'a', 'user'))
        class Down:
            def request(self, path, data):
                raise OSError('connection refused')
        self.assertIn('error', report_installs(Down(), machine, ledger))


if __name__ == '__main__':
    unittest.main()


class SkillNameTest(unittest.TestCase):
    def test_no_generated_skill_name_starts_with_mike(self):
        from learning_audit.promote import recipe_skill
        name, _ = recipe_skill({'id': 'x', 'description': 'd', 'cwd': '/c', 'contract': 'k'})
        self.assertFalse(name.startswith('mike-'), name)
        source = (Path(__file__).parent / 'promote.py').read_text()
        self.assertNotIn("'mike-shared", source)
        self.assertNotIn("'mike-tool", source)


class LessonQualityTest(unittest.TestCase):
    DATA = {
        'lesson_shape': [
            {'shape': 'over_length', 'lessons': 2, 'stored_chars': 2400, 'over_600_chars': 2, 'under_80_chars': 0},
            {'shape': 'rule', 'lessons': 8, 'stored_chars': 1200, 'over_600_chars': 0, 'under_80_chars': 1}],
        'lesson_shape_injected': [
            {'shape': 'over_length', 'injections': 10, 'injected_chars': 24000},
            {'shape': 'rule', 'injections': 30, 'injected_chars': 6000}],
        'lesson_shape_pending': [{'shape': 'over_length', 'lessons': 2, 'oldest_found_at': '2026-10-14'}],
        'lesson_shape_resolved': [{'resolution': 'rewritten', 'lessons': 3}],
        'lesson_relations_write_time': [
            {'kind': 'supersedes', 'method': 'write_time', 'status': 'confirmed', 'relations': 2, 'applied': 1}],
        'lesson_replacement_effect': [
            {'side': 'replaced', 'turns': 40, 'corrected_turns': 4},
            {'side': 'replacement', 'turns': 25, 'corrected_turns': 1}],
    }

    def test_report_shows_counts_by_shape_and_the_share_of_injected_characters(self):
        from learning_audit.report import render_lesson_quality
        text = '\n'.join(render_lesson_quality(self.DATA))
        self.assertIn('Active lessons: 10; mean length 360 characters; 2 over 600 characters; 1 under 80 characters.', text)
        self.assertIn('| over_length | 2 | 2400 | 10 | 24000 | 80.0% |', text)
        self.assertIn('| rule | 8 | 1200 | 30 | 6000 | 20.0% |', text)
        self.assertIn('Waiting for a rewrite: 2 `over_length` lessons', text)
        self.assertIn('Resolved this window: 3 rewritten.', text)
        self.assertIn('`write_time` `supersedes` `confirmed`: 2 (1 applied by consolidation).', text)
        self.assertIn('Turns that received the replaced lesson: 40, 4 corrected (10.0%).', text)
        self.assertIn('Turns that received the replacement lesson: 25, 1 corrected (4.0%).', text)

    def test_report_says_so_when_the_migration_has_not_run(self):
        from learning_audit.report import render_lesson_quality
        text = '\n'.join(render_lesson_quality({'lesson_shape': None}))
        self.assertIn('Not available', text)
        self.assertEqual(render_lesson_quality({}), [])

    def test_a_missing_table_is_reported_as_none_and_any_other_failure_is_raised(self):
        from learning_audit import report
        with patch.object(report, 'query', side_effect=RuntimeError('relation "memory.lesson_shape_findings" does not exist')):
            self.assertIsNone(report.query_if_migrated('select 1'))
        with patch.object(report, 'query', side_effect=RuntimeError('syntax error')):
            with self.assertRaises(RuntimeError):
                report.query_if_migrated('select 1')


class PlacementReportTest(unittest.TestCase):
    DATA = {
        'lesson_placement': [
            {'placement': 'memory', 'lessons': 20, 'agent_scoped': 3, 'naming_a_document': 1},
            {'placement': 'rule', 'lessons': 2, 'agent_scoped': 0, 'naming_a_document': 0},
            {'placement': 'skill', 'lessons': 8, 'agent_scoped': 1, 'naming_a_document': 2},
            {'placement': 'unplaced', 'lessons': 5, 'agent_scoped': 0, 'naming_a_document': 0}],
        'lesson_placement_stuck': [{'lessons': 1}],
        'lesson_placement_proposals': [
            {'id': 41, 'placement': 'rule', 'project': '/p/mike', 'agent': None,
             'text': 'Number every migration above the newest one on main.', 'covered_by': None, 'reason': 'It applies to migration files'}],
        'project_names': [{'project_uuid': 'u1', 'project_path': '/p/mike'}, {'project_uuid': 'u2', 'project_path': '/p/other'}],
        'skill_cap_setting': [{'value': 2}],
    }

    def render(self, data=None):
        from learning_audit import report
        ledger = {'u1:alpha': {'retired_at': None}, 'u1:bravo': {'retired_at': None}, 'u1:charlie': {'retired_at': None},
                  'u1:old': {'retired_at': '2026-10-01'}, 'u2:delta': {'retired_at': None}}
        with patch.object(report, 'skill_ledger', return_value=ledger):
            return '\n'.join(report.render_placement(self.DATA if data is None else data))

    def test_report_counts_lessons_by_placement_and_lists_what_could_not_be_placed(self):
        text = self.render()
        self.assertIn('| memory | 20 | 3 | 1 |', text)
        self.assertIn('| skill | 8 | 1 | 2 |', text)
        self.assertIn('| unplaced | 5 | 0 | 0 |', text)
        self.assertIn('Active lessons: 35; 1 could not be placed after three attempts.', text)

    def test_report_shows_skills_per_project_against_the_cap(self):
        text = self.render()
        self.assertIn('cap of 2 per project', text)
        self.assertIn('| /p/mike | 3 | 2 | over the cap |', text)
        self.assertIn('| /p/other | 1 | 2 | within the cap |', text)

    def test_report_lists_each_rule_proposal_in_full(self):
        text = self.render()
        self.assertIn('### Proposed rule entry: lesson 41 (/p/mike)', text)
        self.assertIn('Number every migration above the newest one on main.', text)
        self.assertIn('Why: It applies to migration files.', text)

    def test_report_says_so_when_the_migration_has_not_run(self):
        text = self.render({'lesson_placement': None})
        self.assertIn('Not available', text)
        self.assertEqual(self.render({}), '')


class SkillBudgetTest(unittest.TestCase):
    def test_reworded_topics_share_a_purpose_and_a_single_shared_word_does_not(self):
        from learning_audit.skill_budget import same_purpose
        self.assertTrue(same_purpose('deploying-the-proxy', 'proxy-deploy-steps'))
        self.assertTrue(same_purpose('database-migrations', 'database-migration'))
        self.assertFalse(same_purpose('database-migrations', 'migration-rollback'))
        self.assertFalse(same_purpose('testing', 'testing-flaky-db-tests'))
        self.assertFalse(same_purpose('', 'the-of'))

    def test_the_cap_does_not_touch_a_project_within_it(self):
        from learning_audit.skill_budget import enforce_cap
        ledger = {'p:a': {'first_emitted': '2026-09-01'}, 'p:b': {'first_emitted': '2026-09-01'}}
        self.assertEqual(enforce_cap(ledger, {}, date(2026, 10, 14), {'p:a', 'p:b'}, set(), {}, 2), ([], []))

    def test_ties_on_loads_retire_the_oldest_and_projects_are_counted_separately(self):
        from learning_audit.skill_budget import enforce_cap
        ledger = {'p:a': {'first_emitted': '2026-09-02'}, 'p:b': {'first_emitted': '2026-09-01'},
                  'p:c': {'first_emitted': '2026-10-14'}, 'q:a': {'first_emitted': '2026-09-01'}}
        retire, hold = enforce_cap(ledger, {}, date(2026, 10, 14), set(ledger), {'p:c'}, {'p:c': 2}, 2)
        self.assertEqual((retire, hold), (['p:b'], []))

    def test_a_reviewed_skill_displaces_a_generated_one_even_a_young_one(self):
        from learning_audit.skill_budget import enforce_cap
        ledger = {'p:gen': {'first_emitted': '2026-10-13'}, 'p:old': {'first_emitted': '2026-09-01'},
                  'p:new': {'first_emitted': '2026-10-14'}}
        reviewed = {'p:old', 'p:new'}
        retire, hold = enforce_cap(ledger, {'p:gen': 5}, date(2026, 10, 14), set(ledger), {'p:new'}, {}, 2, reviewed)
        self.assertEqual((retire, hold), (['p:gen'], []))
        retire, hold = enforce_cap(ledger, {}, date(2026, 10, 14), set(ledger), {'p:new', 'p:gen'}, {}, 2, {'p:new'})
        self.assertEqual((retire, hold), (['p:old'], []))


class LessonDeliveryTest(unittest.TestCase):
    ARMS = {'arms': [
        {'arm': 'mod', 'sessions': 3, 'operator_turns': 10, 'lessons_delivered': 25, 'lessons_per_turn': 2.5,
         'requests': 400, 'usage_requests': 390, 'input_tokens': 100, 'cache_read_tokens': 800,
         'cache_write_tokens': 100, 'cache_read_share': 0.8, 'judged_turns': 8, 'corrected_turns': 2,
         'corrected_rate': 0.25},
        {'arm': 'control', 'sessions': 1, 'operator_turns': 0, 'lessons_delivered': 0, 'lessons_per_turn': None,
         'requests': 50, 'usage_requests': 0, 'input_tokens': 0, 'cache_read_tokens': 0,
         'cache_write_tokens': 0, 'cache_read_share': None, 'judged_turns': 0, 'corrected_turns': 0,
         'corrected_rate': None}]}

    def test_every_rate_is_shown_with_the_counts_it_comes_from(self):
        from learning_audit import delivery
        text = '\n'.join(delivery.render(self.ARMS))
        self.assertIn('| mod | 3 | 10 | 2.50 (25 lessons / 10 turns) | 80.0% (800 / 1000 input tokens) '
                      '| 25.0% (2 corrected / 8 judged) |', text)
        self.assertIn('| control | 1 | 0 | no data (0 lessons / 0 turns) | no data (0 / 0 input tokens) '
                      '| no data (0 corrected / 0 judged) |', text)
        self.assertIn('- mod: 390 of 400 requests carried token usage.', text)

    def test_an_empty_window_and_a_missing_route_are_said_plainly(self):
        from learning_audit import delivery
        empty = {'arms': [dict(self.ARMS['arms'][0], sessions=0), dict(self.ARMS['arms'][1], sessions=0)]}
        self.assertIn('No session in either arm made a request', '\n'.join(delivery.render(empty)))
        self.assertIn('Not available', '\n'.join(delivery.render(None)))

    def test_the_report_carries_the_section_only_when_the_audit_fetched_it(self):
        from learning_audit.report import render
        base = {'totals': {'sessions': 0, 'requests': 0, 'tool_calls': 0, 'missing_usage': 0},
                'start': 'a', 'end': 'b', 'findings': [], 'data': {'proposal_jobs': [], 'proposal_recovery': [],
                'proposer_budget': [], 'governor_settings': [], 'created_tools': []},
                'projects': {}, 'opportunities': []}
        with patch('learning_audit.report.skill_ledger', return_value={}), \
                patch('learning_audit.report.skill_manifest_names', return_value=set()):
            self.assertNotIn('Lesson delivery', render(base))
            self.assertIn('## Lesson delivery: mike mod against proxy injection',
                          render({**base, 'lesson_delivery': self.ARMS}))

    def test_a_proxy_without_the_route_gives_none_and_any_other_failure_is_raised(self):
        import urllib.error
        from learning_audit import delivery
        import io
        missing = urllib.error.HTTPError('u', 404, 'Not Found', {}, io.BytesIO())
        broken = urllib.error.HTTPError('u', 500, 'Server Error', {}, io.BytesIO())
        self.addCleanup(missing.close)
        self.addCleanup(broken.close)
        with patch.object(delivery.urllib.request, 'urlopen', side_effect=missing):
            self.assertIsNone(delivery.fetch('2026-10-07T00:00:00+00:00', '2026-10-14T00:00:00+00:00'))
        with patch.object(delivery.urllib.request, 'urlopen', side_effect=broken):
            with self.assertRaises(urllib.error.HTTPError):
                delivery.fetch('2026-10-07T00:00:00+00:00', '2026-10-14T00:00:00+00:00')
