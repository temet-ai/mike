import copy
import pathlib
import sys
import unittest
from datetime import datetime, timezone

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
from compressor_eval.followups import analyze, lane
from compressor_eval.qualification import qualifies, policy_hash


class Followups(unittest.TestCase):
    def test_lanes_deduplication_and_censored_windows(self):
        source = dict(session_id=1, agent_instance_id=2, claude_code_agent_id='worker',
                      origin='subagent', id=10, wire_id='read', file_path='a.py', content='hidden',
                      result_at='2026-10-18T10:00:00Z')
        cut = dict(source, created_at='2026-10-18T10:00:01Z')
        call = dict(source, id=11, created_at='2026-10-18T10:00:02Z', tool_name='Read')
        foreign = dict(call, id=12, agent_instance_id=3)
        sources = {(lane(source), 'read'): source}
        args = ([cut, cut], {1:'forward'}, [call, foreign], sources,
                lambda _: ('a.py', None), lambda content, _: (content, ''),
                lambda _: [], lambda _: '.py')
        until = datetime(2026, 10, 18, 11, tzinfo=timezone.utc).timestamp()
        table, gaps = analyze(*args, until)
        self.assertEqual(table[('.py','forward')]['reread'], 1)
        self.assertEqual(table[('.py','forward')]['cuts'], 1)
        self.assertEqual(gaps, {})
        table, gaps = analyze(*args, until - 3590)
        self.assertEqual(dict(table), {})
        self.assertEqual(gaps['right_censored'], 1)

    def test_overlapping_sources_count_one_follow_up_and_unknown_lanes_are_gaps(self):
        first = dict(session_id=1, agent_instance_id=2, claude_code_agent_id='worker',
                     origin='subagent', id=10, wire_id='one', file_path='a.py', content='hidden',
                     result_at='2026-10-18T10:00:00Z')
        second = dict(first, id=11, wire_id='two')
        cuts = [dict(s, created_at='2026-10-18T10:00:01Z') for s in (first, second)]
        unknown = dict(cuts[0], agent_instance_id=None, claude_code_agent_id=None, wire_id='three')
        reread = dict(first, id=12, created_at='2026-10-18T10:00:05Z', tool_name='Read')
        sources = {(lane(s), s['wire_id']): s for s in (first, second)}
        until = datetime(2026, 10, 18, 11, tzinfo=timezone.utc).timestamp()
        table, gaps = analyze(cuts + [unknown], {1:'forward'}, [reread], sources,
                              lambda _: ('a.py', None), lambda content, _: (content, ''),
                              lambda _: [], lambda _: '.py', until)
        row = table[('.py','forward')]
        self.assertEqual(row['cuts'], 2)
        self.assertEqual(row['reread'], 1)
        self.assertEqual(row['exposures_with_reread'], 2)
        self.assertEqual(gaps, {'missing_agent_identity': 1})

    def test_unknown_identities_do_not_become_a_shared_lane(self):
        self.assertIsNone(lane({'session_id':1}))
        self.assertIsNone(lane({'session_id':1, 'origin':'subagent'}))
        self.assertIsNotNone(lane({'session_id':1, 'origin':'primary'}))


class Qualification(unittest.TestCase):
    def evidence(self):
        run = dict(execution_kind='actual', execution_ref='fixture-execution', correct=True,
                   verdict_ref='fixture-grader', started_at='2026-10-18T10:00:00Z',
                   finished_at='2026-10-18T10:01:00Z', calls=[{'id':'read','content':'x'}],
                   model_turns=[dict(id='turn', input_tokens=1, output_tokens=1)],
                   task_hash='task', ground_truth_hash='gold', artifact_hash='tree',
                   model_config_hash='model', activation_ref='fixture-activation',
                   constraint_verdict_ref='fixture-constraints')
        return dict(schema='compressor-qualification-v1', policy_sha256=policy_hash({'version':3}),
                    completed_at='2026-10-18T11:00:00Z', recovery=dict(passed=True,
                    weakened_control_failed=True, evidence_ref='fixture-recovery'),
                    pairs=[dict(task_id=f'{family}-{i}', family=family,
                                original=copy.deepcopy(run), compact=copy.deepcopy(run))
                           for family in ('documentation','configuration','test-output') for i in range(3)])

    def test_only_complete_current_matching_evidence_qualifies(self):
        now = datetime(2026,10,18,12,tzinfo=timezone.utc)
        e = self.evidence()
        self.assertTrue(qualifies(e, {'version':3}, now))
        self.assertFalse(qualifies(e, {'version':4}, now))
        self.assertFalse(qualifies(e, {'version':3}, datetime(2026,10,26,tzinfo=timezone.utc)))
        self.assertFalse(qualifies(None, {'version':3}, now))
        for field in ('correct', 'activation_ref', 'constraint_verdict_ref'):
            bad = copy.deepcopy(e)
            bad['pairs'][0]['compact'][field] = False
            self.assertFalse(qualifies(bad, {'version':3}, now))
        e['recovery']['passed'] = False
        self.assertFalse(qualifies(e, {'version':3}, now))


if __name__ == '__main__':
    unittest.main()
