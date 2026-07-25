import importlib.util
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "scripts" / "release_slo.py"
SPEC = importlib.util.spec_from_file_location("release_slo", SCRIPT)
slo = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(slo)


class ReleaseSloTest(unittest.TestCase):
    def calculate(self, previous):
        start = datetime(2026, 1, 1, tzinfo=timezone.utc)
        source = "a" * 40
        history = []
        releases = []
        for index, seconds in enumerate(previous):
            created = start + timedelta(days=index)
            sha = "%040x" % (index + 1)
            history.append({
                "name": "Release Tracky",
                "event": "push",
                "head_sha": sha,
                "created_at": created.isoformat().replace("+00:00", "Z"),
                "updated_at": (created + timedelta(seconds=seconds))
                .isoformat()
                .replace("+00:00", "Z"),
            })
            releases.append({
                "draft": False,
                "prerelease": False,
                "target_commitish": sha,
            })
        current = {
            "created_at": start.isoformat().replace("+00:00", "Z"),
            "head_sha": source,
        }
        jobs = [{
            "completed_at": (start + timedelta(seconds=600))
            .isoformat()
            .replace("+00:00", "Z")
        }]
        return slo.release_slo(history, releases, current, jobs, source)

    def test_defers_percentiles_until_ten_production_releases(self):
        self.assertEqual(
            self.calculate([500] * 8),
            {
                "schema_version": 1,
                "sample_count": 9,
                "assessed": False,
                "target_seconds": {"minimum": 600, "maximum": 900},
            },
        )

    def test_reports_nearest_rank_p95_after_ten_releases(self):
        value = self.calculate([600, 610, 620, 630, 640, 650, 660, 670, 680])
        self.assertEqual(value["sample_count"], 10)
        self.assertTrue(value["assessed"])
        self.assertEqual(value["p50_seconds"], 635.0)
        self.assertEqual(value["p95_seconds"], 680.0)
        self.assertEqual(value["status"], "met")

    def test_current_source_does_not_depend_on_workflow_head(self):
        start = "2026-01-01T00:00:00Z"
        source = "a" * 40
        value = slo.release_slo(
            [{
                "name": "Release Tracky",
                "event": "push",
                "head_sha": source,
                "created_at": start,
                "updated_at": "2026-01-01T00:09:00Z",
            }],
            [{"draft": False, "prerelease": False, "target_commitish": source}],
            {"created_at": start, "head_sha": "b" * 40},
            [{"completed_at": "2026-01-01T00:10:00Z"}],
            source,
        )
        self.assertEqual(value["sample_count"], 1)

    def test_includes_identity_bound_manual_recovery_history(self):
        source = "a" * 40
        value = slo.release_slo(
            [{
                "name": "Release Tracky",
                "event": "workflow_dispatch",
                "display_title": f"Release Tracky {source}",
                "head_sha": "b" * 40,
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:11:00Z",
            }],
            [{"draft": False, "prerelease": False, "target_commitish": source}],
            {"created_at": "2026-01-02T00:00:00Z", "head_sha": "c" * 40},
            [{"completed_at": "2026-01-02T00:10:00Z"}],
            "d" * 40,
        )
        self.assertEqual(value["sample_count"], 2)


if __name__ == "__main__":
    unittest.main()
