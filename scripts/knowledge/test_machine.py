import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import uuid

from knowledge.machine import default_state_dir, machine_identity


class MachineIdentityTests(unittest.TestCase):
    def test_state_dir_uses_xdg_state_home_when_set(self):
        self.assertEqual(default_state_dir({'XDG_STATE_HOME': '/xdg'}, '/home/u'), Path('/xdg/mike'))

    def test_state_dir_falls_back_to_home_when_xdg_is_unset_or_empty(self):
        expected = Path('/home/u/.local/state/mike')
        self.assertEqual(default_state_dir({}, '/home/u'), expected)
        self.assertEqual(default_state_dir({'XDG_STATE_HOME': ''}, '/home/u'), expected)

    def test_state_dir_reads_the_process_environment_by_default(self):
        with patch.dict('os.environ', {'XDG_STATE_HOME': '/from-env'}):
            self.assertEqual(default_state_dir(), Path('/from-env/mike'))
        with patch.dict('os.environ', {'HOME': '/from-home'}, clear=True):
            self.assertEqual(default_state_dir(), Path('/from-home/.local/state/mike'))

    def test_identity_is_created_in_the_default_directory_and_reused(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch.dict('os.environ', {'XDG_STATE_HOME': directory}):
                first = machine_identity()
                second = machine_identity()
            self.assertEqual(first, second)
            stored = json.loads((Path(directory) / 'mike' / 'machine.json').read_text())
            self.assertEqual(stored, first)
            self.assertEqual(str(uuid.UUID(first['machine_uuid'])), first['machine_uuid'])

    def test_identity_never_touches_the_legacy_knowledge_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            legacy = Path(directory) / 'mike-knowledge'
            legacy.mkdir()
            (legacy / 'machine.json').write_text('{"machine_uuid": "legacy", "machine_name": "old"}')
            with patch.dict('os.environ', {'XDG_STATE_HOME': directory}):
                identity = machine_identity()
            self.assertNotEqual(identity['machine_uuid'], 'legacy')
            self.assertEqual(json.loads((legacy / 'machine.json').read_text())['machine_uuid'], 'legacy')


if __name__ == '__main__':
    unittest.main()
