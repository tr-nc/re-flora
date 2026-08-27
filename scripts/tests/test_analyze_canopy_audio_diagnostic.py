from __future__ import annotations

import sys
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import analyze_canopy_audio_diagnostic as analyzer  # noqa: E402


def summary(
    elapsed: float,
    *,
    trees: int = 1,
    emitters: int = 1,
    voices: int = 1,
    samples: int = 8,
    extent_responses: int = 1,
    processed: int = 1,
    retained: int = 0,
    deferred: int = 0,
    rays: int = 16,
) -> str:
    if elapsed < 1.0:
        phase = "Settle"
    elif elapsed < 5.0:
        phase = "ForwardOrbit"
    elif elapsed < 6.0:
        phase = "OcclusionBoundaryHold"
    elif elapsed < 10.0:
        phase = "ReverseOrbit"
    else:
        phase = "Complete"
    return (
        "[AUDIO][CANOPY][SUMMARY] "
        f"time_seconds={elapsed + 1.0:.3f} trajectory_elapsed_seconds={elapsed:.3f} "
        f"trajectory_phase=Some({phase}) trees={trees} emitters={emitters} "
        f"observed_voices={voices} runtime_emitters={emitters} runtime_voices={voices} "
        f"samples={samples} extent_responses={extent_responses} "
        "solve_discards=0 last_discard_spatial_revision=0 last_discard_geometry_version=0 "
        "voice_identity_violations=0 revision_rollbacks=0 sample_contract_violations=0 "
        "aggregate_mismatches=0 petal_superseded_solves=0 telemetry_queue_depth=0 "
        "telemetry_queue_high_water=1 telemetry_drops=0 "
        f"direct_rays={rays} sample_cache_hits=0 processed_extents={processed} lobes=3 "
        f"retained={retained} deferred={deferred} render_rejected_rollbacks=0"
    )


def sample(
    elapsed: float,
    sample_id: int,
    gain: float,
    *,
    tree: int = 0,
    generation: int = 1,
    emitter: str = "emitter-0",
    voice: int = 0,
    status: str = "Solved",
    membership: bool = True,
    rays: int = 8,
    response_revision: int = 10,
) -> str:
    return (
        "[AUDIO][CANOPY][SAMPLE] "
        f"time_seconds={elapsed + 1.0:.3f} tree={tree} generation={generation} "
        f"sample={sample_id} emitter={emitter} voice=Some({voice}) "
        "position_tree_voxels=Vec3(1.0, 2.0, 3.0) position_world=Vec3(1.0, 1.0, 1.0) "
        "observed_world=Some(Vec3(1.0, 1.0, 1.0)) clearance_voxels=3.0 weight=0.125 "
        "observed_weight=Some(0.125) lifecycle_power=1.0 content_seed=7 phase=0.5 "
        "provenance=LeafPlacement wind_target=0.1 wind_filtered=0.1 volume_db=0.0 "
        f"candidate_membership=Some({str(membership).lower()}) solve_status=Some({status}) "
        "hit=Some(false) hit_material=None transmission=Some([1.0, 1.0, 1.0]) "
        f"visible_fraction=Some(1.0) raw_gain=Some([{gain}, {gain}, {gain}]) "
        f"filtered_gain=Some([{gain}, {gain}, {gain}]) classification=Some(Visible) "
        f"dwell_seconds=Some(1.0) rays=Some({rays}) cache_hits=Some(0) hit_count=Some(0) "
        "cache_age_seconds=Some(0.0) spatial_revision=Some(10) geometry_version=Some(20) "
        f"response_spatial_revision=Some({response_revision}) response_geometry_version=Some(20) "
        "lobes=Some(3) direct_transitions=Some(0) direct_superseded=Some(0)"
    )


def single_fixture(*, hold_gain: float = 0.8) -> str:
    points = [
        (1.5, 0.5),
        (2.5, 0.7),
        (3.5, 0.8),
        (4.5, 0.9),
        (5.2, 0.8),
        (5.5, hold_gain),
        (6.5, 0.9),
        (7.5, 0.8),
        (8.5, 0.7),
        (9.5, 0.5),
    ]
    lines: list[str] = []
    for revision, (elapsed, gain) in enumerate(points, start=1):
        lines.append(summary(elapsed, extent_responses=revision, processed=revision, rays=16 * revision))
        lines.extend(sample(elapsed, sample_id, gain) for sample_id in range(8))
    lines.append("Application exited successfully")
    return "\n".join(lines)


class CanopyAudioDiagnosticAnalyzerTests(unittest.TestCase):
    def test_accepts_symmetric_single_voice_distributed_canopy(self) -> None:
        metric = analyzer.analyze_text(single_fixture())

        self.assertTrue(metric.accepted, metric.failures)
        self.assertEqual(metric.mode, "single")
        self.assertEqual(metric.sample_count, 8)
        self.assertAlmostEqual(metric.total_power, 1.0)

    def test_rejects_hold_segment_binary_gain_jump(self) -> None:
        metric = analyzer.analyze_text(single_fixture(hold_gain=0.05))

        self.assertFalse(metric.accepted)
        self.assertTrue(any("hold" in failure for failure in metric.failures))

    def test_accepts_bounded_multitree_retained_and_deferred_routes(self) -> None:
        lines = [
            summary(
                7.0,
                trees=5,
                emitters=5,
                voices=5,
                samples=40,
                extent_responses=5,
                processed=2,
                retained=1,
                deferred=2,
                rays=32,
            )
        ]
        statuses = ["Solved", "Solved", "Retained", "Deferred", "Deferred"]
        for tree, status in enumerate(statuses):
            for sample_id in range(8):
                lines.append(
                    sample(
                        7.0,
                        sample_id,
                        0.6,
                        tree=tree,
                        generation=tree + 1,
                        emitter=f"emitter-{tree}",
                        voice=tree,
                        status=status,
                        membership=status == "Solved",
                        rays=8 if status == "Solved" else 0,
                        response_revision=10 if status == "Solved" else 9,
                    )
                )
        lines.append("Application exited successfully")

        metric = analyzer.analyze_text("\n".join(lines))

        self.assertTrue(metric.accepted, metric.failures)
        self.assertEqual(metric.mode, "budget")
        self.assertGreater(metric.retained_count, 0)
        self.assertGreater(metric.deferred_count, 0)


if __name__ == "__main__":
    unittest.main()
