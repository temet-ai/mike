import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]


class SourceEnvironmentTests(unittest.TestCase):
    def test_exported_database_and_runtime_values_win_even_when_empty(self):
        with tempfile.TemporaryDirectory() as tmp:
            dotenv = pathlib.Path(tmp) / '.env'
            dotenv.write_text('MIKE_DATABASE_URL=file-db\nMIKE_TEST_ADMIN_URL=file-test\nMIKE_LISTEN_ADDR=file-bind\nMIKE_UPSTREAM_URL=file-upstream\n')
            result = subprocess.run([
                'bash', '-euc', 'source "$1"; mike_source_env_preserving_exported "$2"; printf "%s\\n" "$MIKE_DATABASE_URL" "$MIKE_TEST_ADMIN_URL" "$MIKE_LISTEN_ADDR" "$MIKE_UPSTREAM_URL"',
                'test', str(ROOT / 'scripts/lib/source-env.sh'), str(dotenv)
            ], env={'PATH': '/usr/bin:/bin', 'MIKE_DATABASE_URL': 'explicit-db', 'MIKE_TEST_ADMIN_URL': '', 'MIKE_LISTEN_ADDR': 'explicit-bind'}, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, 'explicit-db\n\nexplicit-bind\nfile-upstream\n')

    def test_all_three_development_entrypoints_use_the_preserving_helper(self):
        for filename in ['scripts/cx', 'scripts/start-proxy.sh', 'scripts/deploy-proxy.sh']:
            source = (ROOT / filename).read_text()
            self.assertIn('mike_source_env_preserving_exported ', source)
            self.assertNotIn('source "${PROJECT_ROOT}/.env"', source)


if __name__ == '__main__':
    unittest.main()
