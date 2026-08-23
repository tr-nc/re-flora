import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "terrain_connectivity_perf.py"
SPEC = importlib.util.spec_from_file_location("terrain_connectivity_perf", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class TerrainConnectivityPerfTests(unittest.TestCase):
    def test_percentiles_are_interpolated(self) -> None:
        self.assertEqual(MODULE.distribution([1, 2, 3, 4])["p95_us"], 3.85)

    def test_parser_separates_event_and_post_frames_and_checks_atomicity(self) -> None:
        marker = "[PERF][TERRAIN_CONNECTIVITY_BENCH]"
        lines = [
            f"{marker} phase=event mode=correct frame=10 available_particles=0 total_us=100 current_path_us=0 primary_readback_us=5 classification_us=50 sampling_us=0 invalidation_us=30 publication_us=15 particle_spawn_us=0 classified_voxels=437205 invalidated_voxels=437205 sampled_voxels=0 spawned_particles=0 revision_before=2 revision_after=3",
            f"{marker} phase=frame frame=9 relative=-1 cpu_total_us=4 gpu_present_us=1 tracked_us=1 untracked_us=2 terrain_collider_pending=0 contree_cache_pending=0 water_source_pending=0 water_collider_pending=0 water_cache_pending=0 ddgi_ready=true visible_revision=2",
            f"{marker} phase=frame frame=10 relative=0 cpu_total_us=104 gpu_present_us=1 tracked_us=1 untracked_us=102 terrain_collider_pending=2 contree_cache_pending=1 water_source_pending=0 water_collider_pending=0 water_cache_pending=0 ddgi_ready=false visible_revision=3",
            f"{marker} phase=frame frame=11 relative=1 cpu_total_us=5 gpu_present_us=1 tracked_us=1 untracked_us=3 terrain_collider_pending=0 contree_cache_pending=0 water_source_pending=0 water_collider_pending=0 water_cache_pending=0 ddgi_ready=true visible_revision=3",
            f"{marker} phase=gpu_frame frame=9 relative=-1 frame_render_us=2 tracer_render_us=1 scopes=3 dropped=0",
            f"{marker} phase=gpu_frame frame=10 relative=0 frame_render_us=2 tracer_render_us=1 scopes=3 dropped=0",
            f"{marker} phase=gpu_frame frame=11 relative=1 frame_render_us=3 tracer_render_us=1 scopes=3 dropped=0",
            f"{marker} phase=summary mode=correct event_frame=10 observed_frames=1 remaining_fixture_voxels=0 disposition=detached invalidated_voxels=437205 spawned_particles=0 revision_before=2 revision_after=3 high_water_terrain_collider=2 high_water_contree_cache=1 high_water_water_source=0 high_water_water_collider=0 high_water_water_cache=0 ddgi_ready=true",
        ]
        result = MODULE.summarize_run("\n".join(lines), "correct", 0)
        self.assertEqual(result["frame_cpu"]["event_us"], 104)
        self.assertEqual(result["queues"]["terrain_collider_pending"]["high_water"], 2)
        self.assertEqual(
            result["queues"]["terrain_collider_pending"]["drained_by_relative_frame"], 1
        )
        self.assertTrue(result["atomic_visibility"]["pre_frames_old_revision"])
        self.assertTrue(result["atomic_visibility"]["event_and_post_frames_final_revision"])
        self.assertEqual(result["atomic_visibility"]["final_fixture_voxels"], 0)


if __name__ == "__main__":
    unittest.main()
