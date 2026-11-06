import importlib.machinery
import importlib.util
import pathlib
import unittest
import sys

PATH = pathlib.Path(__file__).resolve().parent.parent / "compressor-loop"
sys.path.insert(0, str(PATH.parent))
loader = importlib.machinery.SourceFileLoader("compressor_loop", str(PATH))
spec = importlib.util.spec_from_loader("compressor_loop", loader)
loop = importlib.util.module_from_spec(spec)
loader.exec_module(loop)


class HiddenText(unittest.TestCase):
    def test_numbered_lines_outside_the_kept_ranges_are_hidden(self):
        content = "\n".join(f"{n:>6}\tline {n}" for n in range(1, 11))
        hidden, shown = loop.hidden_text(content, [(1, 2), (9, 10)])
        self.assertIn("line 5", hidden)
        self.assertNotIn("line 1\n", hidden + "\n")
        self.assertIn("line 9", shown)

    def test_a_superseded_read_hides_everything(self):
        hidden, shown = loop.hidden_text("     1\ta\n     2\tb", None)
        self.assertEqual(hidden, "a\nb")
        self.assertEqual(shown, "")


class Decide(unittest.TestCase):
    def setUp(self):
        self.settings = {"file_read_compression.protected_paths": ["**/CLAUDE.md"],
                         "file_read_compression.forward_share": 50}
        loop.setting = self.settings.get

    def table(self, forward_hidden, control_hidden, cuts=40):
        return {(".md", "forward"): {"cuts": cuts, "edit_hidden": forward_hidden, "reread": 0},
                (".md", "control"): {"cuts": cuts, "edit_hidden": control_hidden, "reread": 0}}

    def test_a_type_harmed_beyond_the_control_is_protected(self):
        decisions = loop.decide(self.table(4, 1), [], [], 1.0)
        self.assertEqual(decisions[0]["action"], "protect")
        self.assertIn("**/*.md", decisions[0]["value"])
        self.assertIn("**/CLAUDE.md", decisions[0]["value"])

    def test_harm_matching_the_control_is_not_acted_on(self):
        self.assertEqual(loop.decide(self.table(3, 3), [], [], 1.0), [])

    def test_too_few_reads_is_not_acted_on(self):
        self.assertEqual(loop.decide(self.table(10, 0, cuts=10), [], [], 1.0), [])

    def test_a_dearer_forward_arm_stops_forwarding(self):
        decisions = loop.decide({}, [200.0] * 40, [100.0] * 40, 0.01)
        self.assertEqual(decisions, [dict(action="stop", key="file_read_compression.forward_share",
                                          value=0, why="forward arm costs more per turn (p=0.010)")])

    def test_lower_cost_cannot_widen_without_task_and_recovery_evidence(self):
        decisions = loop.decide({}, [80.0] * 40, [100.0] * 40, 0.01)
        self.assertEqual(decisions, [])

    def test_evidence_defaults_to_the_saved_qualification_file(self):
        import json, tempfile
        with tempfile.TemporaryDirectory() as tmp:
            state = pathlib.Path(tmp)
            self.assertIsNone(loop.read_evidence([], state))
            (state / "qualification.json").write_text(json.dumps({"verdict": "qualified"}))
            self.assertEqual(loop.read_evidence([], state), {"verdict": "qualified"})
            other = state / "other.json"
            other.write_text(json.dumps({"verdict": "x"}))
            self.assertEqual(loop.read_evidence(["--evidence", str(other)], state), {"verdict": "x"})

    def test_an_unclear_difference_changes_nothing(self):
        self.assertEqual(loop.decide({}, [80.0] * 40, [100.0] * 40, 0.4), [])

    def test_policy_protection_prevents_widening_in_the_same_batch(self):
        from unittest.mock import patch
        with patch.object(loop, 'qualifies', return_value=True):
            self.assertEqual(loop.decide({}, [80.0] * 40, [100.0] * 40, 0.01)[0]['action'], 'widen')
            decisions = loop.decide(self.table(4, 0), [80.0] * 40, [100.0] * 40, 0.01)
            self.assertEqual([d['action'] for d in decisions], ['protect'])

    def test_the_permutation_test_separates_clear_from_noise(self):
        self.assertLess(loop.permutation_p([200.0 + i for i in range(40)], [100.0 + i for i in range(40)]), 0.01)
        self.assertGreater(loop.permutation_p([100.0 + i for i in range(40)], [100.0 + i for i in range(40)]), 0.5)


class Coverage(unittest.TestCase):
    def test_missing_usage_is_excluded_instead_of_treated_as_free(self):
        self.assertEqual(loop.per_turn({1:'forward'}, {1:dict(fresh=0, cache_write=0,
                         cache_read=0, output=0, complete_usage=False)}, {1:3}, 'forward'), [])

    def test_usage_rolls_up_per_session_and_per_subagent_lane(self):
        row = lambda agent, n, complete=True: dict(session_id=1, agent_instance_id=None, claude_code_agent_id=agent,
                                                   origin='subagent' if agent else 'primary', fresh=n,
                                                   cache_write=0, cache_read=0, output=0, complete_usage=complete)
        cost, lanes = loop.roll_up([row(None, 10), row('a', 5), row('b', 7, complete=False)])
        self.assertEqual(cost[1]['fresh'], 22)
        self.assertFalse(cost[1]['complete_usage'])
        self.assertEqual(set(lanes), {(1, None, 'a'), (1, None, 'b')})
        self.assertEqual(loop.per_lane({1: 'forward'}, lanes, 'forward'), [5.0])

    def test_arm_changes_across_daily_chunks_are_excluded(self):
        from unittest.mock import patch
        start = (loop.datetime.now(loop.timezone.utc) - loop.timedelta(days=1, hours=1)).isoformat()
        rows = [[dict(session_id=1, arm='control', observed_arms=1)],
                [dict(session_id=1, arm='forward', observed_arms=1), dict(session_id=2, arm='control', observed_arms=1)]]
        with patch.object(loop, 'run_sql', side_effect=rows):
            arms, gaps = loop.load_arms(start)
        self.assertEqual(arms, {2:'control'})
        self.assertEqual(gaps, {'mixed_arm_sessions':1})


DETAIL = ("kept output lines 1-20,50-53,281-300 (44 of 300 lines, 1 signal lines) "
          "in place of 2 omitted range(s); the exact original is recoverable")


class ShellKeptRanges(unittest.TestCase):
    def test_ranges_are_parsed_from_the_detail(self):
        self.assertEqual(loop.parse_shell_cut({"detail": DETAIL}), [(1, 20), (50, 53), (281, 300)])

    def test_a_single_line_is_a_range_of_one(self):
        self.assertEqual(loop.parse_shell_cut({"detail": "kept output lines 7,9-9 (2 of 30 lines)"}), [(7, 7), (9, 9)])

    def test_a_detail_that_does_not_parse_is_none(self):
        self.assertIsNone(loop.parse_shell_cut({"detail": "file a.rs: kept file lines 1-2 and 3-4"}))
        self.assertIsNone(loop.parse_shell_cut({"detail": None}))

    def test_the_stage_name_reaches_the_arm_query(self):
        from unittest.mock import patch
        with patch.object(loop, 'run_sql', return_value=[]) as sql:
            self.assertEqual(loop.load_arms('2026-11-01T00:00:00+00:00', 'shell_output_compression'),
                             ({}, {'mixed_arm_sessions': 0}))
        self.assertIn("st->>'stage' = 'shell_output_compression'", sql.call_args_list[0].args[0])


CONTENT = "\n".join([
    "cargo test output start",                      # 1 kept
    "kept_line_token_alpha ok",                     # 2 kept
    "error in crates/mike/src/hidden_file.rs",      # 3 hidden
    "kept_line_token_alpha repeats",                # 4 hidden, but the token is also kept
    "failing commit deadbeef12",                    # 5 hidden
    "ran cmd_only_token_x",                         # 6 hidden, but the command names it
    "short ab",                                     # 7 hidden, too short to be a token
    "tail one",                                     # 8 kept
    "tail two",                                     # 9 kept
])
COMMAND = "cargo test --lib cmd_only_token_x"
KEPT = [(1, 2), (8, 9)]
T0 = "2026-11-01T10:00:00+00:00"
LATER = loop.stamp("2026-11-02T00:00:00+00:00")


def shell_cut(**over):
    return dict(session_id=1, agent_instance_id=None, claude_code_agent_id=None, origin='primary',
                created_at=T0, wire_id='toolu_1', detail="kept output lines 1-2,8-9 (4 of 9 lines)", **over)


def shell_source(content=CONTENT, **over):
    return {((1, None, None), 'toolu_1'): dict(id=10, content=content, result_at="2026-11-01T10:00:01+00:00",
                                               input={"command": COMMAND}, **over)}


def call(cid, minutes, text, session=1, agent=None):
    return dict(id=cid, session_id=session, created_at=f"2026-11-01T10:{minutes:02d}:00+00:00", tool_name="Read",
                input={"file_path": text}, agent_instance_id=None, claude_code_agent_id=agent,
                origin='subagent' if agent else 'primary')


class ShellNeededLater(unittest.TestCase):
    def table(self, calls, arm='forward', until=LATER, cuts=None):
        return loop.shell_harm_table(cuts or [shell_cut()], {1: arm}, shell_source(), calls, until)

    def test_output_is_split_on_one_based_line_numbers(self):
        hidden, shown = loop.split_output(CONTENT, KEPT)
        self.assertIn("hidden_file.rs", hidden)
        self.assertNotIn("hidden_file.rs", shown)
        self.assertIn("tail two", shown)
        parts = [{"type": "text", "text": "a\nb"}, {"type": "text", "text": "\nc"}]
        self.assertEqual(loop.split_output(parts, [(2, 2)]), ("a\nc", "b"))
        self.assertEqual(loop.split_output(None, [(1, 1)]), (None, None))

    def test_only_tokens_the_kept_lines_and_command_lack_are_needed(self):
        hidden, shown = loop.split_output(CONTENT, KEPT)
        self.assertEqual(loop.needed_tokens(hidden, shown, COMMAND),
                         {"crates/mike/src/hidden_file.rs", "deadbeef12"})

    def test_a_later_call_using_a_hidden_token_counts(self):
        table, gaps = self.table([call(20, 5, "crates/mike/src/hidden_file.rs")])
        self.assertEqual((table['forward']['cuts'], table['forward']['needed_later']), (1, 1))
        self.assertEqual(table['control']['cuts'], 0)
        self.assertEqual(gaps, {})

    def test_a_token_already_in_the_kept_lines_does_not_count(self):
        table, _ = self.table([call(20, 5, "kept_line_token_alpha"), call(21, 6, "cmd_only_token_x")])
        self.assertEqual((table['forward']['cuts'], table['forward']['needed_later']), (1, 0))

    def test_calls_out_of_window_or_lane_do_not_count(self):
        elsewhere = [call(20, 5, "deadbeef12", agent="sub"), call(21, 5, "deadbeef12", session=2),
                     call(22, 45, "deadbeef12")]
        table, _ = self.table(elsewhere)
        self.assertEqual(table['forward']['needed_later'], 0)
        table, _ = self.table([call(23, 0, "deadbeef12")])  # not after the receipt
        self.assertEqual(table['forward']['needed_later'], 0)

    def test_the_arm_decides_the_row_and_a_wire_id_counts_once(self):
        table, _ = self.table([call(20, 5, "deadbeef12")], arm='control', cuts=[shell_cut(), shell_cut()])
        self.assertEqual((table['control']['cuts'], table['control']['needed_later']), (1, 1))

    def test_a_right_censored_cut_is_left_out(self):
        table, gaps = self.table([call(20, 5, "deadbeef12")], until=loop.stamp("2026-11-01T10:10:00+00:00"))
        self.assertEqual(table['forward']['cuts'], 0)
        self.assertEqual(gaps, {'right_censored': 1})

    def test_a_cut_without_a_source_is_a_gap(self):
        table, gaps = loop.shell_harm_table([shell_cut()], {1: 'forward'}, {}, [], LATER)
        self.assertEqual(table['forward']['cuts'], 0)
        self.assertEqual(gaps, {'missing_source_or_policy': 1})


class ShellDecide(unittest.TestCase):
    def setUp(self):
        self.settings = {"shell_output_compression.forward_share": 50}
        loop.setting = self.settings.get

    def table(self, forward, control, cuts=40):
        return {"forward": {"cuts": cuts, "needed_later": forward}, "control": {"cuts": cuts, "needed_later": control}}

    def test_a_dearer_forward_arm_stops_shell_forwarding(self):
        decisions = loop.decide_shell([200.0] * 40, [100.0] * 40, 0.01, {})
        self.assertEqual(decisions, [dict(action="stop", key="shell_output_compression.forward_share",
                                          value=0, why="forward arm costs more per turn (p=0.010)")])

    def test_hidden_output_needed_beyond_the_control_stops_it(self):
        decisions = loop.decide_shell([], [], 1.0, self.table(6, 1))
        self.assertEqual([d['action'] for d in decisions], ['stop'])
        self.assertEqual(decisions[0]['value'], 0)
        self.assertIn("needed later +12.5% over control", decisions[0]['why'])

    def test_nothing_changes_without_evidence(self):
        self.assertEqual(loop.decide_shell([80.0] * 40, [100.0] * 40, 0.01, {}), [])  # cheaper: never widened
        self.assertEqual(loop.decide_shell([200.0] * 40, [100.0] * 40, 0.4, {}), [])  # unclear difference
        self.assertEqual(loop.decide_shell([200.0] * 5, [100.0] * 5, 0.01, {}), [])   # too few sessions
        self.assertEqual(loop.decide_shell([], [], 1.0, self.table(3, 3)), [])         # harm matches control
        self.assertEqual(loop.decide_shell([], [], 1.0, self.table(10, 0, cuts=10)), [])  # too few cuts

    def test_a_share_already_zero_proposes_nothing(self):
        self.settings["shell_output_compression.forward_share"] = 0
        self.assertEqual(loop.decide_shell([200.0] * 40, [100.0] * 40, 0.01, self.table(6, 1)), [])

    def test_without_shell_arms_the_scorecard_is_one_line(self):
        from unittest.mock import patch
        with patch.object(loop, 'load_arms', return_value=({}, {})):
            self.assertEqual(loop.shell_scorecard('2026-11-01T00:00:00+00:00'), ([loop.NO_SHELL_ARMS], []))


if __name__ == "__main__":
    unittest.main()
