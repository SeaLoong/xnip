"""Pure-stdlib contracts for the native E2E result gates."""

import subprocess
import sys
import textwrap
import unittest

from native_e2e import require_tap, require_unittest


def tap(*, tests=2, passed=2, failed=0, cancelled=0, skipped=0, todo=0):
    return textwrap.dedent(f"""\
        TAP version 13
        # Subtest: actual tool fixture
        ok 1 - actual tool fixture
        1..{tests}
        # tests {tests}
        # suites 0
        # pass {passed}
        # fail {failed}
        # cancelled {cancelled}
        # skipped {skipped}
        # todo {todo}
        # duration_ms 527.931355
    """)


class NativeE2eResultContract(unittest.TestCase):
    def child_unittest(self, body):
        source = "import unittest\n" + textwrap.dedent(body) + "\nunittest.main()\n"
        result = subprocess.run([sys.executable, "-c", source], capture_output=True,
                                text=True, timeout=10)
        return result, result.stdout + result.stderr

    def test_real_unittest_success_with_application_ok_output(self):
        result, output = self.child_unittest("""
            class Actual(unittest.TestCase):
                def test_tool(self):
                    print('OK')
                    self.assertEqual(2 + 2, 4)
        """)
        self.assertEqual(result.returncode, 0, output)
        self.assertEqual(require_unittest(output, 1),
                         {"tests": 1, "pass": 1, "fail": 0, "skipped": 0})

    def test_real_unittest_skip_with_fake_ok_is_rejected(self):
        result, output = self.child_unittest("""
            class Actual(unittest.TestCase):
                @classmethod
                def setUpClass(cls):
                    print('OK')
                @unittest.skip('the tool body stays unexecuted')
                def test_tool(self):
                    raise AssertionError('this body must stay unexecuted')
        """)
        self.assertEqual(result.returncode, 0, output)
        self.assertIn('OK (skipped=1)', output)
        self.assertTrue(output.startswith('OK\n'), output)
        with self.assertRaises(AssertionError):
            require_unittest(output, 1)

    def test_real_unittest_expected_failure_with_fake_ok_is_rejected(self):
        result, output = self.child_unittest("""
            class Actual(unittest.TestCase):
                @unittest.expectedFailure
                def test_tool(self):
                    print('OK')
                    self.fail('expected failure is a separate test outcome')
        """)
        self.assertEqual(result.returncode, 0, output)
        self.assertIn('expected failures=1', output)
        with self.assertRaises(AssertionError):
            require_unittest(output, 1)

    def test_real_unittest_unexpected_success_is_rejected(self):
        result, output = self.child_unittest("""
            class Actual(unittest.TestCase):
                @unittest.expectedFailure
                def test_tool(self):
                    print('OK')
        """)
        self.assertEqual(result.returncode, 1, output)
        self.assertIn('unexpected successes=1', output)
        with self.assertRaises(AssertionError):
            require_unittest(output, 1)

    def test_unittest_requires_one_final_summary_and_exact_count(self):
        normal = 'SDK diagnostics\nRan 2 tests in 6.208s\n\nOK\n'
        self.assertEqual(require_unittest(normal.replace('\n', '\r\n'), 2)['pass'], 2)
        for output, expected in [('', 2), (normal, 3), (normal + normal, 2),
                                 (normal + 'late diagnostics\n', 2),
                                 ('Ran 2 tests in 0.001s\n\nOK (skipped=1)\nOK\n', 2)]:
            with self.subTest(output=output, expected=expected), self.assertRaises(AssertionError):
                require_unittest(output, expected)

    def test_tap_accepts_complete_node_summary_and_crlf(self):
        expected = {"tests": 2, "pass": 2, "fail": 0, "skipped": 0}
        self.assertEqual(require_tap(tap(), 2), expected)
        self.assertEqual(require_tap(tap().replace('\n', '\r\n'), 2), expected)

    def test_tap_rejects_conflicting_or_repeated_counts(self):
        for output in [tap() + tap(passed=1, skipped=1), tap() + tap(),
                       '# tests 2\n# pass 2\n# fail 0\n# skipped 0\n' + tap(passed=1, skipped=1)]:
            with self.subTest(output=output), self.assertRaises(AssertionError):
                require_tap(output, 2)

    def test_tap_requires_every_terminal_counter(self):
        for name in ('tests', 'suites', 'pass', 'fail', 'cancelled', 'skipped', 'todo'):
            output = '\n'.join(line for line in tap().splitlines()
                               if not line.startswith('# ' + name + ' ')) + '\n'
            with self.subTest(name=name), self.assertRaises(AssertionError):
                require_tap(output, 2)

    def test_tap_rejects_incomplete_or_nonterminal_summary(self):
        for output in ['', tap().replace('# duration_ms 527.931355\n', ''),
                       tap() + 'late diagnostics\n', tap().replace('# fail 0\n', '# fail malformed\n')]:
            with self.subTest(output=output), self.assertRaises(AssertionError):
                require_tap(output, 2)

    def test_tap_rejects_skips_failures_cancellations_todos_and_count_mismatch(self):
        for kwargs in [{'passed': 1, 'skipped': 1}, {'passed': 1, 'failed': 1},
                       {'passed': 1, 'cancelled': 1}, {'passed': 1, 'todo': 1}, {'tests': 3}]:
            with self.subTest(kwargs=kwargs), self.assertRaises(AssertionError):
                require_tap(tap(**kwargs), 2)


if __name__ == '__main__':
    unittest.main()
