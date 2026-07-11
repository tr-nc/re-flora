#!/usr/bin/env python3
"""Unit tests for parse_perf_log.py."""

from __future__ import annotations

import importlib.util
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).with_name("parse_perf_log.py")
SPEC = importlib.util.spec_from_file_location("parse_perf_log", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
parse_perf_log = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = parse_perf_log
SPEC.loader.exec_module(parse_perf_log)


class ParsePerfLogTests(unittest.TestCase):
    def parse_fixture(self):
        return parse_perf_log.parse_log_lines(
            [
                "[12:00:00 INFO src::app::core] [PERF] frame 30 total 16.20ms egui 1.20ms gpu+present 5.00ms",
                "[12:00:00 INFO src::app::core] [PERF][FRAME] frame 31 total 17.00ms egui 1.10ms gpu_present 5.20ms contree_poll 0.01ms terrain_source 0.02ms deferred_rebuild 0.03ms cache_queue 0.04ms collider_queue 0.05ms water_edit_soak 0.06ms water_handoff 0.07ms particles 0.08ms tracked_cpu 0.36ms untracked_cpu 10.34ms queues deferred_pending=3 deferred_active=1 deferred_inflight=true source_pending=4 source_active=2 collider_pending=5 collider_active=0 collider_inflight=false cache_pending=6 cache_active=1 cache_inflight=true",
                "[12:00:00 INFO re_flora_water::mls_mpm] [PERF][WATER] particles 10000 grid UVec3(160, 64, 160) nodes 1638400 substeps 12 total 42.00ms avg 3.500ms/substep repair 0.00ms clear 0.40ms p2g 12.00ms grid 1.20ms grid_update 1.10ms g2p 28.00ms g2p_gather 5.00ms g2p_box 1.00ms g2p_terrain 2.00ms g2p_repair 0.50ms diagnostics 0.20ms residual 0.10ms shadow_measure 0.30ms p2g_density_corr/substep 10.5 p2g_density_corr_factor_avg 1.125 p2g_density_corr_factor_max 1.750 terrain_cache_skips/substep 120 terrain_cache_projections/substep 30 terrain_exact_fallbacks/substep 4 terrain_exact_checks/substep 5 terrain_exact_corrections/substep 2 terrain_shadow_samples/substep 25.5 terrain_shadow_false_skips 1 terrain_shadow_sdf_err_avg 0.01234 terrain_shadow_sdf_err_max 0.05678 active_nodes/substep 900 particle_y 0.100..1.200 avg 0.400 terrain_sdf_min -0.0100 penetrating 3 no_sdf 7",
                "[12:00:00 INFO src::app::core::water::runtime] [PERF][WATER_THREAD] seconds=1.010 enabled=true particles=10000 ticks=60 active_ticks=50 idle_ticks=10 commands=12 commands_per_tick=0.20 max_commands_per_tick=5 maxed_command_ticks=1 command_drain=0.700ms publish_count=20 publish_particles=50000 publish_particles_per_publish=2500.0 publish=1.500ms publish_lock=0.050ms snapshot_bucket_count=4",
                "[12:00:00 INFO src::app::core::particles] [PERF][PARTICLES] alive=13 snapshots=4109 water_debug=4096 emitters butterflies=1 leaves=36 tick_step=true dt=0.0439 total=0.148ms setup=0.001ms emit=0.011ms sim=0.001ms collect=0.000ms plan=0.016ms snapshot=0.021ms upload=0.098ms",
                "[12:00:00 INFO src::app::core] [PERF][GPU_FRAME_SCOPE] frame 30 scopes=3 dropped=0 frame.render=9000us tracer.render=7000us egui.render=500us",
                "[12:00:00 INFO src::builder::surface] [PERF][GPU_JOB_SCOPE] name=surface.build queue=Graphics chunk UVec3(0, 0, 0) duration=615us",
                "[12:00:00 INFO src::app::core::terrain_rebuild] [PERF][SYNC_VISIBLE_REBUILD] chunks 8 total 13.67ms preserve_flora=false chunk_ids=[]",
                "[12:00:00 INFO src::app::core::terrain_rebuild] [PERF][DEFERRED_REBUILD] chunk UVec3(0, 0, 0) total 6.00ms wall 10.00ms surface_total 4.00ms contree_total 1.00ms scene_total 0.50ms scene_finish 0.20ms active_voxels 10 remaining 0 revision 1 latest=true preserve_flora=false",
                "[12:00:00 INFO src::app::core::terrain_rebuild] [PERF][DEFERRED_REBUILD_PHASE] chunk UVec3(0, 0, 0) revision 1 phase surface_finish main_thread 0.30ms surface_total 4.00ms active_voxels 10 flora 0.10ms preserve_flora 0.20ms place_flora false",
                "[12:00:00 INFO src::builder::surface] [PERF][SURFACE_BUILD] chunk UVec3(0, 0, 0) total 4.00ms fence_latency 3.00ms flora 0.10ms",
                "[12:00:00 INFO src::builder::contree] [QUEUE][CONTREE_REBUILD] chunk UVec3(0, 0, 0) total_ms=1.50ms fence_latency_ms=0.50ms size_ms=0.10ms confirm_ms=0.20ms",
                "[12:00:00 INFO src::app::core::water] refreshed GPU solid source chunk UVec3(0, 0, 0) total=2.00ms gpu_sample_total=1.00ms fence_latency=0.40ms gpu_submit=0.20ms gpu_readback=0.30ms",
                "[12:00:00 INFO src::app::core::water] built collider chunk UVec3(0, 0, 0) build_ms=3.00",
                "[12:00:00 INFO src::app::core::water] applied worker grid cache region chunk=IVec3(0, 0, 0) worker_ms=4.00 apply_ms=0.50",
                "[12:00:00 INFO src::app::core::water] discarded stale worker grid cache region chunk=IVec3(0, 0, 0) worker_ms=5.00",
            ],
            source="fixture.log",
        )

    def metric(self, summary, group_key: str, metric: str):
        return summary.groups[group_key].metrics[metric]

    def test_parses_existing_perf_markers(self):
        summary = self.parse_fixture()

        self.assertEqual(self.metric(summary, "frame_basic", "cpu_other"), [10.0])
        self.assertEqual(self.metric(summary, "frame_detail", "water_handoff"), [0.07])
        self.assertEqual(self.metric(summary, "frame_queues", "deferred_pending"), [3.0])
        self.assertEqual(self.metric(summary, "frame_queues", "deferred_inflight"), [1.0])
        self.assertEqual(self.metric(summary, "frame_queues", "collider_inflight"), [0.0])
        self.assertEqual(self.metric(summary, "water_counts", "particles"), [10000.0])
        self.assertEqual(self.metric(summary, "water", "avg_substep"), [3.5])
        self.assertEqual(self.metric(summary, "water_diagnostics", "terrain_cache_skips_per_substep"), [120.0])
        self.assertEqual(self.metric(summary, "water_diagnostics", "terrain_shadow_sdf_err_max"), [0.05678])
        self.assertEqual(self.metric(summary, "water_diagnostics", "terrain_penetrating"), [3.0])
        self.assertEqual(self.metric(summary, "water_thread_values", "enabled"), [1.0])
        self.assertEqual(self.metric(summary, "water_thread_values", "commands_per_tick"), [0.2])
        self.assertEqual(self.metric(summary, "water_thread", "publish"), [1.5])
        self.assertEqual(self.metric(summary, "particles", "upload"), [0.098])
        self.assertEqual(self.metric(summary, "particle_counts", "water_debug"), [4096.0])
        self.assertEqual(self.metric(summary, "gpu_frame_scope", "tracer.render"), [7.0])
        self.assertEqual(self.metric(summary, "gpu_job_scope", "surface.build"), [0.615])
        self.assertEqual(self.metric(summary, "terrain_events", "terrain_sync_visible_rebuild"), [13.67])
        self.assertEqual(self.metric(summary, "terrain_events", "water_cache_discard_worker"), [5.0])

    def test_output_formats_are_machine_readable(self):
        summary = self.parse_fixture()

        json_buffer = io.StringIO()
        parse_perf_log.write_json(summary, json_buffer)
        payload = json.loads(json_buffer.getvalue())
        self.assertEqual(payload["source"], "fixture.log")
        self.assertTrue(any(row["metric"] == "tracer.render" for row in payload["metrics"]))

        csv_buffer = io.StringIO()
        parse_perf_log.write_csv(summary, csv_buffer)
        csv_text = csv_buffer.getvalue()
        self.assertIn("group,metric,unit,n,sum,mean,median,p95,max", csv_text)
        self.assertIn("GPU frame scopes,tracer.render,ms,1,7.0", csv_text)

    def test_latest_log_prefers_pointer_file(self):
        with tempfile.TemporaryDirectory() as tmp_dir_name:
            tmp_dir = Path(tmp_dir_name)
            older = tmp_dir / "re-flora-old.log"
            newer = tmp_dir / "re-flora-new.log"
            pointed = tmp_dir / "re-flora-pointed.log"
            older.write_text("old")
            newer.write_text("new")
            pointed.write_text("pointed")
            (tmp_dir / parse_perf_log.LATEST_POINTER_FILE).write_text(f"{pointed}\n")

            self.assertEqual(parse_perf_log.latest_log(tmp_dir), pointed)


if __name__ == "__main__":
    unittest.main()
