import unittest

from scripts.check_ddgi_indirect_response import analyze


def fixture():
    events = []
    serial = 0
    for phase, start, values in [
        ("baseline", 0, [0.0, 0.0, 0.0]),
        ("on-editing", 1000, [0.0, 0.3, 0.8]),
        ("on-settling", 2000, [0.98, 1.0, 1.02]),
        ("off-editing", 3000, [1.0, 0.7, 0.2]),
        ("off-settling", 4000, [0.001, 0.0, 0.0]),
    ]:
        if phase != "baseline":
            events.append({"event": "phase", "phase": phase, "ms": start})
        if phase.endswith("editing"):
            events.extend(
                {
                    "event": "edit",
                    "phase": phase,
                    "edit": i,
                    "ms": start + i * 10,
                    "geometry_revision": serial + i + 2,
                }
                for i in range(1, 41)
            )
        for i, value in enumerate(values):
            serial += 1
            events.append(
                {
                    "event": "sample",
                    "phase": phase,
                    "ms": start + i * 100,
                    "serial": serial,
                    "field_serial": serial,
                    "ready": True,
                    "rgb": [value] * 3,
                    "weight": 1.0,
                    "probes": 4,
                    "geometry_revision": 2,
                    "radiance_revision": 1,
                }
            )
    events.append({"event": "phase", "phase": "done", "ms": 5000})
    return events


class IndirectResponseTests(unittest.TestCase):
    def test_real_on_and_off_response(self):
        report = analyze(fixture())
        self.assertEqual(report["failures"], [])
        self.assertEqual(report["on_ms"]["10_percent"], 100)
        self.assertEqual(report["off_ms"]["10_percent"], 100)

    def test_black_rgb_cannot_pass_on_advancing_identities(self):
        events = fixture()
        for event in events:
            if event["event"] == "sample":
                event["rgb"] = [0.0] * 3
        self.assertTrue(analyze(events)["failures"])

    def test_frozen_lit_rgb_cannot_pass_removal(self):
        events = fixture()
        for event in events:
            if event["event"] == "sample" and event["phase"].startswith("off"):
                event["rgb"] = [1.0] * 3
        self.assertTrue(analyze(events)["failures"])

    def test_response_only_after_editing_fails(self):
        events = fixture()
        for event in events:
            if event["event"] == "sample" and event["phase"] == "on-editing":
                event["rgb"] = [0.0] * 3
        self.assertTrue(analyze(events)["failures"])

    def test_global_sky_or_unsupported_receiver_is_not_ddgi_evidence(self):
        events = fixture()
        for event in events:
            if event["event"] == "sample":
                event["probes"] = 0
        self.assertTrue(analyze(events)["failures"])

    def test_nan_or_missing_completion_fails(self):
        events = fixture()
        events[-2]["rgb"] = [float("nan")] * 3
        self.assertTrue(analyze(events)["failures"])
        self.assertTrue(analyze(fixture()[:-1])["failures"])

    def test_static_control_preserves_response_contract(self):
        events = [event for event in fixture() if event["event"] != "edit"]
        self.assertEqual(analyze(events, static_control=True)["failures"], [])
        self.assertTrue(analyze(fixture(), static_control=True)["failures"])

    def test_edits_must_change_actual_geometry(self):
        events = fixture()
        for event in events:
            if event["event"] == "edit":
                event["geometry_revision"] = 2
        self.assertTrue(analyze(events)["failures"])

    def test_repeated_stale_field_is_not_three_reference_observations(self):
        events = fixture()
        for event in events:
            if event["event"] == "sample" and event["phase"] == "on-settling":
                event["field_serial"] = 6
        self.assertTrue(analyze(events)["failures"])

    def test_unstable_reference_fails(self):
        events = fixture()
        samples = [
            event
            for event in events
            if event["event"] == "sample" and event["phase"] == "on-settling"
        ]
        samples[-1]["rgb"] = [10.0] * 3
        self.assertTrue(analyze(events)["failures"])


if __name__ == "__main__":
    unittest.main()
