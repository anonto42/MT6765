"""Unit test of walpad-keys.py action dispatch (launch: allowlist, run: unchanged).  No device needed."""
import importlib.util, json, os, tempfile, unittest

SRC = os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../config/rootfs/usr/local/bin/walpad-keys.py")
ETC = os.path.join(os.path.dirname(SRC), "../../../etc/walpad")


def load(apps_path):
    spec = importlib.util.spec_from_file_location("walpad_keys", SRC)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    m.APPS_FILE = apps_path
    m.spawned, m.shelled, m.logged = [], [], []
    m.spawn = lambda cmd, env: m.spawned.append((cmd, env))
    m.sh = lambda *cmd, wait=False: m.shelled.append(list(cmd))
    m.log = lambda *a: m.logged.append(a)
    return m


class Keys(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.TemporaryDirectory()
        self.apps = os.path.join(self.d.name, "apps.json")
        json.dump([{"id": "browser", "exec": ["firefox-esr", "--new-instance", "about:blank"], "env": {"X": "1"}},
                   {"id": "bad", "exec": "rm -rf /"}], open(self.apps, "w"))
        self.m = load(self.apps)

    def tearDown(self): self.d.cleanup()

    def test_launch_known_id_runs_exactly_exec_with_app_env(self):
        self.m.do("launch:browser")
        self.assertEqual(len(self.m.spawned), 1)
        cmd, env = self.m.spawned[0]
        self.assertEqual(cmd, ["firefox-esr", "--new-instance", "about:blank"])
        self.assertEqual(env["DISPLAY"], ":0"); self.assertEqual(env["HOME"], "/root")
        self.assertEqual(env["MOZ_WEBRENDER"], "0"); self.assertEqual(env["X"], "1")
        self.assertEqual(self.m.shelled, [])

    def test_launch_unknown_id_does_nothing(self):
        for a in ("launch:nope", "launch:", "launch:browser extra", "launch:../bin/sh"):
            self.m.do(a)
        self.assertEqual(self.m.spawned, []); self.assertEqual(self.m.shelled, [])
        self.assertTrue(self.m.logged)

    def test_launch_never_takes_a_command(self):
        self.m.do("launch:rm -rf /"); self.m.do("launch:bad")
        self.assertEqual(self.m.spawned, []); self.assertEqual(self.m.shelled, [])

    def test_launch_missing_or_broken_apps_file(self):
        self.m.APPS_FILE = os.path.join(self.d.name, "missing.json"); self.m.do("launch:browser")
        open(self.apps, "w").write("{not json"); self.m.APPS_FILE = self.apps; self.m.do("launch:browser")
        self.assertEqual(self.m.spawned, [])

    def test_run_still_works(self):
        self.m.do("run:st -f mono"); self.assertEqual(self.m.shelled, [["st", "-f", "mono"]])
        self.assertEqual(self.m.spawned, [])

    def test_shipped_files(self):
        keys = json.load(open(os.path.join(ETC, "keys.json")))
        self.assertEqual(keys["vol_up"]["quad"], "launch:browser")
        m = load(os.path.join(ETC, "apps.json")); m.do(keys["vol_up"]["quad"])
        self.assertEqual(len(m.spawned), 1); self.assertEqual(m.spawned[0][0][0], "firefox-esr")


if __name__ == "__main__":
    unittest.main(verbosity=1)
