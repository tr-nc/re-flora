"""The current diagnostic mode lane is not a shadow source."""

import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import check_tree_branch_lighting as branch
from analyze_environment_irradiance_capture import PIXEL, Capture


class TreeBranchShadowDiagnosticsTests(unittest.TestCase):
    def test_unoccluded_gate_checks_only_sources_for_each_capture_schema(self):
        for version, third, unoccluded in ((10, 1, 1), (10, 0, 0),
                                           (11, 0, 1), (11, 0.5, 1), (11, 1, 1)):
            with self.subTest(version=version, third=third):
                capture = Capture(
                    path=Path("synthetic.rfirr"), version=version, width=1, height=1,
                    backend=1, spacing_voxels=32, debug_view=0, plane_count=5,
                    payload=PIXEL.pack(1, 1, 1, 1),
                    world_payload=PIXEL.pack(1, 0.6, 1, 1),
                    direct_light_payload=PIXEL.pack(1, 1, 1, 1),
                    terrain_shadow_receiver_payload=PIXEL.pack(1, 0.6, 1, 1),
                    direct_sun_shadow_payload=PIXEL.pack(1, 1, third, 1),
                )
                with patch.object(branch, "load_capture", return_value=capture):
                    result = branch.measure(capture.path)
                self.assertEqual(result["unoccluded_samples"], unoccluded)
