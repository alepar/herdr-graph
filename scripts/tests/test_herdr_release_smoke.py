"""Behavioral regressions for the real matrix driver; no network or Cargo builds."""
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import signal
import time
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True

DRIVER = Path(__file__).resolve().parents[1] / 'herdr-release-smoke'


def load_driver():
    loader = importlib.machinery.SourceFileLoader('herdr_release_smoke', str(DRIVER))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class DriverTests(unittest.TestCase):
    def setUp(self):
        self.assertTrue(DRIVER.is_file(), 'matrix driver must exist')
        self.driver = load_driver()
        self.temp = tempfile.TemporaryDirectory(prefix='hgm-unit-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def program(self, name, body):
        binary = self.root / name
        binary.write_text('#!/usr/bin/env python3\n' + body)
        binary.chmod(0o755)
        return binary

    def output(self, result='ok', count=1):
        return (f'running {count} test\ntest core ... {result}\n\n'
                f'test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n')

    def test_exact_complete_result_passes(self):
        self.assertEqual(self.driver.classify_suite(0, self.output(), ['core'])['status'], 'PASS')

    def test_false_pass_outputs_fail(self):
        for code, output in [(1, self.output()), (0, ''), (0, self.output('ignored')),
                             (0, self.output() + 'test extra ... ok\n'),
                             (0, self.output() + 'test core ... ok\n'),
                             (0, self.output().replace('0 ignored', '1 ignored')),
                             (0, self.output() + 'SKIPPED: no herdr\n'),
                             (0, 'test core ... ok\n')]:
            with self.subTest(code=code, output=output):
                self.assertEqual(self.driver.classify_suite(code, output, ['core'])['status'], 'FAIL')

    def test_sigterm_sets_cancellation_and_runs_cleanup(self):
        fake = self.program('sleeping-test', 'import time\ntime.sleep(30)\n')
        result_path = self.root / 'interrupted.json'
        ready = self.root / 'ready'
        helper = self.program('signal-driver', f'''import importlib.machinery, json, os, sys
from pathlib import Path
sys.dont_write_bytecode = True
m = importlib.machinery.SourceFileLoader('signal_matrix', {str(DRIVER)!r}).load_module()
m.install_signal_handlers()
Path({str(ready)!r}).touch()
result = m.run_suite(Path({str(fake)!r}), ['core'], dict(os.environ), Path({str(self.root / 'run')!r}), 20)
Path({str(result_path)!r}).write_text(json.dumps(result))
''')
        child = subprocess.Popen([str(helper)], env=self.driver.safe_environment(),
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 3
            while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                time.sleep(.02)
            self.assertTrue(ready.exists(), 'actual driver signal handler must install before running')
            child.send_signal(signal.SIGTERM)
            output = child.communicate(timeout=5)
            self.assertEqual(child.returncode, 0, output)
            result = json.loads(result_path.read_text())
            self.assertEqual(result['status'], 'FAIL')
            self.assertTrue(result['cancelled'])
            self.assertTrue(result['cleanup']['clean'])
        finally:
            if child.poll() is None:
                child.kill()
            child.communicate(timeout=5)

    def test_timeout_fails_even_with_success_output(self):
        fake = self.program('late', f'print({self.output()!r}, flush=True)\nimport time\ntime.sleep(20)\n')
        result = self.driver.run_suite(fake, ['core'], dict(os.environ), self.root / 'run', 0.15)
        self.assertEqual(result['status'], 'FAIL')
        self.assertTrue(result['timed_out'])

    def test_nested_cargo_fails_even_if_the_fixture_ignores_its_error(self):
        fake = self.program('cargo-caller', "import subprocess,os\nsubprocess.run([os.environ.get('CARGO', '/usr/bin/false')])\n" +
                            f'print({self.output()!r})\n')
        result = self.driver.run_suite(fake, ['core'], dict(os.environ), self.root / 'run', 5)
        self.assertEqual(result['status'], 'FAIL')
        self.assertIn('forbidden tool', result['detail'])

    def test_early_return_without_runtime_fixture_fails(self):
        fake = self.program('early-return', f'print({self.output()!r})\n')
        result = self.driver.run_suite(fake, ['core'], dict(os.environ), self.root / 'run', 5, expected_roots=1)
        self.assertEqual(result['status'], 'FAIL')
        self.assertIn('runtime fixture roots', result['detail'])

    def test_process_inspection_without_own_identity_fails_closed(self):
        from types import SimpleNamespace
        with patch.object(self.driver.subprocess, 'run', return_value=SimpleNamespace(returncode=0, stdout='unparseable')):
            with self.assertRaisesRegex(RuntimeError, 'own process identity'):
                self.driver.process_snapshot()

    def test_nonzero_fake_test_executable_fails(self):
        fake = self.program('bad', f'print({self.output()!r})\nraise SystemExit(3)\n')
        result = self.driver.run_suite(fake, ['core'], dict(os.environ), self.root / 'run', 5)
        self.assertEqual(result['status'], 'FAIL')
        self.assertEqual(result['returncode'], 3)

    def test_executable_case_inventory_must_match(self):
        fake = self.program('inventory', "print('other: test\\n\\n1 test, 0 benchmarks')\n")
        with self.assertRaisesRegex(RuntimeError, 'inventory'):
            self.driver.validate_inventory(fake, ['core'], dict(os.environ))

    def test_wrong_reported_version_fails(self):
        fake = self.program('herdr', "print('herdr 0.9.4')\n")
        with self.assertRaisesRegex(RuntimeError, 'reports'):
            self.driver.verify_binary(fake, '0.9.5', hashlib.sha256(fake.read_bytes()).hexdigest())

    def test_nonzero_version_command_fails(self):
        fake = self.program('herdr', "print('herdr 0.9.5')\nraise SystemExit(1)\n")
        with self.assertRaises(RuntimeError):
            self.driver.verify_binary(fake, '0.9.5', hashlib.sha256(fake.read_bytes()).hexdigest())

    def test_bad_checksum_fails_without_deleting_cache_entry(self):
        fake = self.program('herdr', "print('herdr 0.9.5')\n")
        before = fake.read_bytes()
        with self.assertRaisesRegex(RuntimeError, 'checksum'):
            self.driver.verify_binary(fake, '0.9.5', '0' * 64)
        self.assertEqual(fake.read_bytes(), before)

    def test_unknown_version_cli_fails_before_build(self):
        run = subprocess.run([str(DRIVER), '--versions', '999.0.0'], capture_output=True, text=True)
        self.assertNotEqual(run.returncode, 0)
        self.assertIn('unknown version', run.stderr)

    def test_manifest_rejects_missing_platform_asset(self):
        manifest = self.root / 'releases.tsv'
        manifest.write_text('0.9.5\therdr-macos-aarch64\t' + '0' * 64 + '\n')
        with self.assertRaisesRegex(ValueError, 'missing assets'):
            self.driver.read_list(manifest)

    def test_upstream_missing_asset_is_failure(self):
        payload = [{'tag_name': 'v0.9.6', 'draft': False, 'prerelease': False, 'assets': []}]
        with self.assertRaisesRegex(ValueError, 'missing assets'):
            self.driver.release_metadata(payload)

    def test_upstream_future_stable_and_digest_drift_detected(self):
        def release(version):
            return {'tag_name': 'v' + version, 'assets': [
                {'name': asset, 'digest': 'sha256:' + '1' * 64} for asset in self.driver.ASSETS]}
        remote = self.driver.release_metadata([release('0.9.5'), release('0.10.0'),
                                                release('0.9.0'), release('0.10.1-rc.1')])
        self.assertEqual(list(remote), ['0.9.5', '0.10.0'])
        local = {'0.9.5': dict.fromkeys(self.driver.ASSETS, '0' * 64)}
        problems = self.driver.upstream_problems(local, remote)
        self.assertTrue(any('0.10.0' in problem for problem in problems))
        self.assertTrue(any('digest' in problem for problem in problems))

    def test_sanitized_parent_has_no_credentials_or_live_seat(self):
        env = self.driver.safe_environment({'PATH': '/bin', 'ANTHROPIC_API_KEY': 'secret',
             'OPENAI_API_KEY': 'secret', 'HERDR_SOCKET_PATH': '/live', 'CLAUDE_PID': '1'})
        self.assertEqual(env, {'PATH': '/bin'})

    def test_cleanup_failure_overrides_success(self):
        fake = self.program('ok', f'print({self.output()!r})\n')
        with patch.object(self.driver, 'cleanup_owned', return_value={'clean': False, 'errors': ['survivor']}):
            result = self.driver.run_suite(fake, ['core'], dict(os.environ), self.root / 'run', 5)
        self.assertEqual(result['status'], 'FAIL')

    def test_root_removal_failure_is_recorded_as_failed_cleanup(self):
        private = Path('/private/tmp/hg-' + __import__('uuid').uuid4().hex)
        private.mkdir()
        self.addCleanup(lambda: __import__('shutil').rmtree(private, ignore_errors=True))
        fake = self.program('owned-ok', f'''import json,os
from pathlib import Path
root=Path({str(private)!r})
(root/'.matrix-owner').write_text(os.environ['HG_MATRIX_OWNER'])
with open(os.environ['HG_MATRIX_REGISTRY'],'a') as registry:
    registry.write(json.dumps({{'root':str(root)}})+'\\n')
print({self.output()!r})
''')
        with patch.object(self.driver.shutil, 'rmtree', side_effect=OSError('cannot remove owned root')):
            try:
                result = self.driver.run_suite(fake, ['core'], dict(os.environ), self.root / 'run', 5)
            except OSError as error:
                self.fail(f'cleanup errors must produce a recorded FAIL: {error}')
        self.assertEqual(result['status'], 'FAIL')
        self.assertFalse(result['cleanup']['clean'])
        recorded = json.loads((self.root / 'run/result.json').read_text())
        self.assertEqual(recorded['status'], 'FAIL')

    def test_cleanup_refuses_unowned_registry_root(self):
        root = self.root / 'foreign'
        root.mkdir()
        registry = self.root / 'roots.jsonl'
        registry.write_text(json.dumps({'root': str(root)}) + '\n')
        result = self.driver.cleanup_owned(registry, 'our-token')
        self.assertFalse(result['clean'])
        self.assertTrue(root.exists())

    def test_cleanup_tracks_protected_orphan_before_its_parent_exits(self):
        private = Path('/private/tmp/hg-' + __import__('uuid').uuid4().hex)
        private.mkdir()
        self.addCleanup(lambda: __import__('shutil').rmtree(private, ignore_errors=True))
        daemon_code = ("import subprocess,time; from pathlib import Path; "
                       "p=subprocess.Popen(['/bin/sleep','20']); "
                       f"Path({str(private / 'orphan.pid')!r}).write_text(str(p.pid)); time.sleep(.5)")
        body = f'''import json, os, subprocess, time
from pathlib import Path
root = Path({str(private)!r})
(root / '.matrix-owner').write_text(os.environ['HG_MATRIX_OWNER'])
with open(os.environ['HG_MATRIX_REGISTRY'], 'a') as registry:
    registry.write(json.dumps({{'root': str(root)}}) + '\\n')
subprocess.Popen([{sys.executable!r}, '-c', {daemon_code!r}], env={{'HG_TEST_ROOT': str(root)}}, start_new_session=True)
time.sleep(30)
'''
        fake = self.program('orphan', body)
        result = self.driver.run_suite(fake, ['core'], dict(os.environ), self.root / 'run', 2)
        pid = int((private / 'orphan.pid').read_text())
        self.assertIn(pid, result['cleanup']['reaped'], result['cleanup'])
        self.assertEqual(result['cleanup']['survivors'], [])

    def test_timeout_reaps_detached_child_from_registered_owned_root(self):
        # A child deliberately escapes the runner's process group; only the owned-root fallback finds it.
        private = Path('/private/tmp/hg-' + __import__('uuid').uuid4().hex)
        private.mkdir()
        self.addCleanup(lambda: __import__('shutil').rmtree(private, ignore_errors=True))
        body = f'''import json, os, subprocess, time
from pathlib import Path
root = Path({str(private)!r})
(root / '.matrix-owner').write_text(os.environ['HG_MATRIX_OWNER'])
with open(os.environ['HG_MATRIX_REGISTRY'], 'a') as registry:
    registry.write(json.dumps({{'root': str(root)}}) + '\\n')
child_code = "import subprocess, time; subprocess.Popen(['/bin/sleep', '30']); time.sleep(30)"
child = subprocess.Popen([{sys.executable!r}, '-c', child_code], env={{'HG_TEST_ROOT': str(root)}}, start_new_session=True)
(root / 'child.pid').write_text(str(child.pid))
print({self.output()!r}, flush=True)
time.sleep(30)
'''
        fake = self.program('detached', body)
        result = self.driver.run_suite(fake, ['core'], dict(os.environ), self.root / 'run', 2)
        pid = int((private / 'child.pid').read_text())
        self.assertEqual(result['status'], 'FAIL')
        self.assertEqual(result['cleanup']['survivors'], [], result['cleanup'])
        self.assertIn(pid, result['cleanup']['reaped'])
        self.assertTrue((self.root / 'run' / 'cleanup.json').is_file())


if __name__ == '__main__':
    unittest.main()
