from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts.validate_ddgi_density_lifecycle import (
    DensityLifecycleError,
    validate_density_lifecycle,
)


def line(payload: str) -> str:
    return f"[00:00:00 INFO re_flora] {payload}"


def valid_lines() -> list[str]:
    return [
        line("[DDGI_ACCEPT][DENSITY] checkpoint=baseline field_serial=1 geometry_revision=2 radiance_revision=1 spacing_voxels=32 state=Converging update_epoch=0 source_field_serial=0 source_state=none"),
        line("[DDGI_ACCEPT][DENSITY] checkpoint=density-midflight active_token_serial=1 active_field_serial=1 active_geometry_revision=2 active_spacing_voxels=32 obsolete_density_token_serial=2 obsolete_density_field_serial=3 obsolete_density_spacing_voxels=16 old_field_visible=true active_available=true"),
        line("[DDGI_ACCEPT][DENSITY] checkpoint=geometry-preempted-density obsolete_density_token_serial=2 obsolete_density_field_serial=3 terrain_token_serial=3 target_geometry_revision=3 terrain_spacing_voxels=32 queued_density_spacing_voxels=16 obsolete_density_consumer_visible=false active_available=true"),
        line("[DDGI_ACCEPT][DENSITY] checkpoint=geometry-e0-private terrain_token_serial=3 generation_token_serial=3 obsolete_density_token_serial=2 geometry_revision=3 radiance_revision=1 epoch_zero_field_serial=5 source_field_serial=1 private_current_field_serial=5 private_current_update_epoch=0 active_spacing_voxels=32 queued_density_spacing_voxels=16 obsolete_density_consumer_visible=false active_available=true"),
        line("[DDGI] staging promoted token_serial=3 generation_token_serial=3 kind=Terrain spacing_voxels=32 geometry_revision=3 radiance_revision=1 epoch_zero_field_serial=5 published_field_serial=10 published_update_epoch=5 published_source=Some(DdgiFieldKey { geometry_revision: 3 }) building=Some(DdgiFieldIdentity { spacing_voxels: 32 })"),
        line("[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=3 generation_token_serial=3 geometry_revision=3 radiance_revision=1 spacing_voxels=32 epoch_zero_field_serial=5 published_field_serial=10 update_epoch=5 source=Some(DdgiFieldKey { geometry_revision: 3 })"),
        line("[DDGI_ACCEPT][DENSITY] checkpoint=geometry-recovery-published terrain_token_serial=3 generation_token_serial=3 obsolete_density_token_serial=2 geometry_revision=3 radiance_revision=1 epoch_zero_field_serial=5 published_field_serial=10 published_update_epoch=5 same_generation=true active_spacing_voxels=32 queued_density_spacing_voxels=16 obsolete_density_consumer_visible=false active_available=true"),
        line("[DDGI_ACCEPT][DENSITY] checkpoint=density-retry-midflight active_token_serial=3 active_field_serial=10 active_geometry_revision=3 active_radiance_revision=1 active_spacing_voxels=32 density_token_serial=4 density_field_serial=12 density_radiance_revision=1 density_spacing_voxels=16 progress=512/35937 old_field_visible=true active_available=true"),
        line("[ENV_IRRADIANCE_CAPTURE] checkpoint target=e0 build_token_serial=4 generation_token_serial=4 epoch_zero_field_serial=12 field_serial=12 source_field_serial=0 geometry_revision=3 radiance_revision=1 spacing_voxels=16 state=Converging update_epoch=0 publication=Published"),
        line("[DDGI] staging promoted token_serial=4 generation_token_serial=4 kind=Density spacing_voxels=16 geometry_revision=3 radiance_revision=1 epoch_zero_field_serial=12 published_field_serial=12 published_update_epoch=0"),
        line("[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=4 generation_token_serial=4 geometry_revision=3 radiance_revision=1 spacing_voxels=16 epoch_zero_field_serial=12 published_field_serial=12 update_epoch=0"),
        line("[DDGI_ACCEPT][DENSITY] checkpoint=complete field_serial=12 geometry_revision=3 radiance_revision=1 spacing_voxels=16 state=Converging update_epoch=0 source_field_serial=0 source_state=none"),
        line("[DDGI_ACCEPT][DENSITY] complete obsolete_density_token_serial=2 terrain_token_serial=3 density_token_serial=4 obsolete_density_consumer_visible=false first_consumer_visible_16_epoch=0 geometry_revision=3 radiance_revision=1 spacing_voxels=16"),
    ]


class ValidateDdgiDensityLifecycleTests(unittest.TestCase):
    def validate(self, lines: list[str]) -> dict[str, int]:
        with tempfile.TemporaryDirectory() as directory:
            console = Path(directory) / "console.log"
            console.write_text("\n".join(lines) + "\n")
            return validate_density_lifecycle(console)

    def test_accepts_one_strict_owner_lineage(self) -> None:
        result = self.validate(valid_lines())
        self.assertEqual(result["obsolete_density_token_serial"], 2)
        self.assertEqual(result["terrain_token_serial"], 3)
        self.assertEqual(result["density_token_serial"], 4)
        self.assertEqual(result["geometry_epoch_zero_field_serial"], 5)
        self.assertEqual(result["geometry_published_update_epoch"], 5)
        self.assertEqual(result["field_serial"], 12)
        self.assertEqual(result["build_token_serial"], 4)

    def test_rejects_shuffled_complete_promotion_publication_private_preemption(self) -> None:
        lines = valid_lines()
        shuffled = [lines[0], lines[1], lines[11], lines[4], lines[6], lines[3], lines[2], *lines[7:11], lines[12]]
        with self.assertRaisesRegex(DensityLifecycleError, "out of order"):
            self.validate(shuffled)

    def test_rejects_duplicate_checkpoint(self) -> None:
        lines = valid_lines()
        lines.insert(4, lines[3])
        with self.assertRaisesRegex(DensityLifecycleError, "duplicate"):
            self.validate(lines)

    def test_rejects_mixed_log_identity(self) -> None:
        lines = valid_lines()
        lines[6] = lines[6].replace("terrain_token_serial=3", "terrain_token_serial=33")
        with self.assertRaisesRegex(DensityLifecycleError, "terrain token"):
            self.validate(lines)

    def test_rejects_obsolete_token_promotion_even_between_checkpoints(self) -> None:
        lines = valid_lines()
        lines.insert(3, line("[DDGI] staging promoted token_serial=2 kind=Density spacing_voxels=16 geometry_revision=2 published_update_epoch=0"))
        with self.assertRaisesRegex(DensityLifecycleError, "obsolete density token"):
            self.validate(lines)

    def test_rejects_future_obsolete_promotion_before_its_identity_is_declared(self) -> None:
        lines = valid_lines()
        lines.insert(1, lines[9].replace("token_serial=4", "token_serial=2"))
        with self.assertRaisesRegex(DensityLifecycleError, "promotion"):
            self.validate(lines)

    def test_rejects_mixed_consumer_before_candidate_identity_is_declared(self) -> None:
        lines = valid_lines()
        lines.insert(1, lines[10].replace("active_token_serial=4", "active_token_serial=99"))
        with self.assertRaisesRegex(DensityLifecycleError, "consumer"):
            self.validate(lines)

    def test_rejects_coordinated_geometry_field_rewrite_without_owner_markers(self) -> None:
        lines = valid_lines()
        lines[6] = lines[6].replace("published_field_serial=10", "published_field_serial=999")
        lines[7] = lines[7].replace("active_field_serial=10", "active_field_serial=999")
        with self.assertRaisesRegex(DensityLifecycleError, "field"):
            self.validate(lines)

    def test_rejects_promotion_and_consumer_radiance_rewrite(self) -> None:
        lines = valid_lines()
        lines[4] = lines[4].replace("radiance_revision=1", "radiance_revision=99")
        lines[5] = lines[5].replace("radiance_revision=1", "radiance_revision=99")
        with self.assertRaisesRegex(DensityLifecycleError, "radiance"):
            self.validate(lines)

    def test_every_later_checkpoint_is_bound_to_baseline_radiance(self) -> None:
        mutations = (
            (3, "radiance_revision=1"),
            (6, "radiance_revision=1"),
            (7, "active_radiance_revision=1"),
            (7, "density_radiance_revision=1"),
            (8, "radiance_revision=1"),
            (9, "radiance_revision=1"),
            (10, "radiance_revision=1"),
            (11, "radiance_revision=1"),
            (12, "radiance_revision=1"),
        )
        for index, identity in mutations:
            with self.subTest(index=index, identity=identity):
                lines = valid_lines()
                lines[index] = lines[index].replace(identity, identity.replace("=1", "=99"))
                with self.assertRaisesRegex(DensityLifecycleError, "radiance"):
                    self.validate(lines)

    def test_rejects_final_density_epoch_with_a_source(self) -> None:
        lines = valid_lines()
        lines[11] = lines[11].replace("source_field_serial=0", "source_field_serial=10")
        with self.assertRaisesRegex(DensityLifecycleError, "source"):
            self.validate(lines)

    def test_rejects_geometry_epoch_zero_from_a_nonbaseline_history_source(self) -> None:
        lines = valid_lines()
        lines[3] = lines[3].replace("source_field_serial=1", "source_field_serial=999")
        with self.assertRaisesRegex(DensityLifecycleError, "source"):
            self.validate(lines)


if __name__ == "__main__":
    unittest.main()
