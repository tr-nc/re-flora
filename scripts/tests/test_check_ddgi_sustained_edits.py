import unittest

from scripts.check_ddgi_sustained_edits import analyze_log


def fixture(promotions=((250, 3), (750, 7), (3500, 32), (4300, 42)), end=True):
    events = [(0, "[DDGI_SUSTAINED] begin")]
    events += [(i * 100, f"[DDGI_SUSTAINED] edit={i} revision={i + 2} active_revision=Some(2)")
               for i in range(1, 41)]
    events += [(ms, f"[DDGI] staging promoted geometry_revision={revision} radiance_revision=1")
               for ms, revision in promotions]
    if end:
        events.append((4000, "[DDGI_SUSTAINED] end"))
    return "\n".join(f"[00:00:{ms // 1000:02}.{ms % 1000:03} INFO test] {line}"
                     for ms, line in sorted(events, key=lambda event: event[0]))


class SustainedEditLogTests(unittest.TestCase):
    def test_progress_and_final_catchup(self):
        report = analyze_log(fixture())
        self.assertEqual(report["validation_failures"], [])
        self.assertEqual(report["final_revision"], 42)
        self.assertEqual(report["final_catchup_ms"], 300)
        self.assertEqual(report["first_useful_publication_ms"], 250)
        self.assertEqual(report["publication_gap_ms"]["max"], 2750)
        self.assertEqual(report["max_no_publication_ms"], 2750)

    def test_no_during_edit_progress_is_not_hidden_by_final_publication(self):
        report = analyze_log(fixture(((4300, 42),)))
        self.assertTrue(report["validation_failures"])
        self.assertEqual(report["final_catchup_ms"], 300)

    def test_duplicate_publications_do_not_count_as_progress(self):
        report = analyze_log(fixture(((250, 3), (750, 3), (4300, 42))))
        self.assertTrue(report["validation_failures"])

    def test_missing_final_revision_is_a_failure(self):
        report = analyze_log(fixture(((250, 3), (750, 7), (4300, 41))))
        self.assertTrue(report["validation_failures"])
        self.assertIsNone(report["final_catchup_ms"])

    def test_old_initial_field_is_not_useful_edit_progress(self):
        report = analyze_log(fixture(((50, 2), (250, 3), (4300, 42))))
        self.assertTrue(report["validation_failures"])
        self.assertEqual(report["useful_promotions_during_edits"], [3])

    def test_boundary_gap_is_not_omitted(self):
        report = analyze_log(fixture(((100, 3), (200, 4), (4300, 42))))
        self.assertEqual(report["max_no_publication_ms"], 3800)

    def test_missing_end_or_runtime_error_fails(self):
        self.assertTrue(analyze_log(fixture(end=False))["validation_failures"])
        self.assertTrue(analyze_log(fixture() + "\nthread 'main' panicked")["validation_failures"])

    def test_unrelated_demo_invalidates_receiver_evidence(self):
        log = fixture() + "\n[00:00:00.000 INFO test] [CLIMBING] authored editable inward fixture"
        self.assertIn("unrelated climbing demo changed fixture terrain/camera",
                      analyze_log(log)["validation_failures"])

    def test_repeated_edit_number_fails(self):
        log = fixture().replace("edit=40 ", "edit=39 ")
        self.assertTrue(analyze_log(log)["validation_failures"])


if __name__ == "__main__":
    unittest.main()
