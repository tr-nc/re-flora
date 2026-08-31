from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

import sys


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from ddgi_evidence.model import ScenarioValidation, ValidateScenarioLog  # noqa: E402
from ddgi_evidence.validation import validate_scenario_log  # noqa: E402


SEQUENTIAL_REOPENED = """
[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision=2
[DDGI] runtime observed visible terrain revision=3 invalidation_voxel_bound=Some((UVec3(112, 184, 238), UVec3(224, 276, 366)))
[ENV_LIGHT_EDIT_CYCLE] requested edit=close-skylight source_revision=2 target_revision=3
[DDGI] staging prepared token_serial=2 kind=Terrain spacing_voxels=32 active_terrain_revision=2 target_terrain_revision=3
[DDGI][LOCAL_RECOVERY] prepared geometry_revision=3 dirty_probes=48 preserved_probes=4865 minimum_epoch=4 stable_epochs=2 max_absolute_delta=0.100
[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit=close-skylight target_revision=3
[DDGI] staging promoted token_serial=2 kind=Terrain spacing_voxels=32 geometry_revision=3 radiance_revision=1 published_state=Converging published_update_epoch=5 published_source=Some(field-7)
[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=2 geometry_revision=3 radiance_revision=1 spacing_voxels=32 state=Converging update_epoch=5
[ENV_LIGHT_EDIT_CYCLE] edited probe field ready edit=close-skylight terrain_revision=3
[DDGI] runtime observed visible terrain revision=4 invalidation_voxel_bound=Some((UVec3(112, 184, 238), UVec3(224, 276, 366)))
[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision=3 target_revision=4
[DDGI] staging prepared token_serial=3 kind=Terrain spacing_voxels=32 active_terrain_revision=3 target_terrain_revision=4
[DDGI][LOCAL_RECOVERY] prepared geometry_revision=4 dirty_probes=48 preserved_probes=4865 minimum_epoch=4 stable_epochs=2 max_absolute_delta=0.100
[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit=reopen-skylight target_revision=4
[DDGI] staging promoted token_serial=3 kind=Terrain spacing_voxels=32 geometry_revision=4 radiance_revision=1 published_state=Converging published_update_epoch=5 published_source=Some(field-14)
[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=3 geometry_revision=4 radiance_revision=1 spacing_voxels=32 state=Converging update_epoch=5
[DDGI][FLORA_CONSUMER] draw_recorded active_token_serial=3 terrain_revision=4 spacing_voxels=32 instance_count=99
[ENV_LIGHT_EDIT_CYCLE] edited probe field ready edit=reopen-skylight terrain_revision=4
[DDGI] staging prepared token_serial=4 kind=Density spacing_voxels=32 active_terrain_revision=4 target_terrain_revision=4
[ENV_LIGHT_EDIT_CYCLE] requested density rebuild terrain_revision=4 spacing_voxels=32
[DDGI] staging promoted token_serial=4 kind=Density spacing_voxels=32 geometry_revision=4 radiance_revision=1 published_state=Converging published_update_epoch=0 published_source=None
[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=4 geometry_revision=4 radiance_revision=1 spacing_voxels=32 state=Converging update_epoch=0
[DDGI][FLORA_CONSUMER] draw_recorded active_token_serial=4 terrain_revision=4 spacing_voxels=32 instance_count=99
[ENV_LIGHT_EDIT_CYCLE] density rebuild ready terrain_revision=4
[ENV_LIGHT_EDIT_CYCLE] complete mode=reopened final_terrain_revision=4
[ENV_IRRADIANCE_CAPTURE] checkpoint target=e8 build_token_serial=4 field_serial=25 state=Converging update_epoch=8 publication=Published
[ENV_IRRADIANCE_CAPTURE] saved geometry_revision=4 radiance_revision=1 spacing_voxels=32 build_token_serial=4 field_serial=25
[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run
""".strip()


CLOSED = """
[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision=2
[DDGI] runtime observed visible terrain revision=3 invalidation_voxel_bound=Some((UVec3(112, 184, 238), UVec3(224, 276, 366)))
[ENV_LIGHT_EDIT_CYCLE] requested edit=close-skylight source_revision=2 target_revision=3
[DDGI] staging prepared token_serial=2 kind=Terrain spacing_voxels=32 active_terrain_revision=2 target_terrain_revision=3
[DDGI][LOCAL_RECOVERY] prepared geometry_revision=3 dirty_probes=48 preserved_probes=4865 minimum_epoch=4 stable_epochs=2 max_absolute_delta=0.100
[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit=close-skylight target_revision=3
[DDGI] staging promoted token_serial=2 kind=Terrain spacing_voxels=32 geometry_revision=3 radiance_revision=1 published_state=Converging published_update_epoch=5 published_source=Some(field-7)
[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=2 geometry_revision=3 radiance_revision=1 spacing_voxels=32 state=Converging update_epoch=5
[ENV_LIGHT_EDIT_CYCLE] edited probe field ready edit=close-skylight terrain_revision=3
[ENV_LIGHT_EDIT_CYCLE] complete mode=closed final_terrain_revision=3
[ENV_IRRADIANCE_CAPTURE] checkpoint target=converged build_token_serial=2 field_serial=12 state=Converged update_epoch=8 publication=Published
[ENV_IRRADIANCE_CAPTURE] saved geometry_revision=3 radiance_revision=1 spacing_voxels=32 build_token_serial=2 field_serial=12
[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run
""".strip()


INFLIGHT_FINAL = """
[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision=2
[DDGI] runtime observed visible terrain revision=3 invalidation_voxel_bound=Some((UVec3(112, 184, 238), UVec3(224, 276, 366)))
[ENV_LIGHT_EDIT_CYCLE] requested edit=close-skylight source_revision=2 target_revision=3
[DDGI] staging prepared token_serial=2 kind=Terrain spacing_voxels=32 active_terrain_revision=2 target_terrain_revision=3
[DDGI][LOCAL_RECOVERY] prepared geometry_revision=3 dirty_probes=48 preserved_probes=4865 minimum_epoch=4 stable_epochs=2 max_absolute_delta=0.100
[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit=close-skylight target_revision=3
[ENV_LIGHT_EDIT_INFLIGHT] obsolete candidate observed terrain_revision=3 active_terrain_revision=Some(2) token_serial=2 spacing_voxels=32
[DDGI] runtime observed visible terrain revision=4 invalidation_voxel_bound=Some((UVec3(112, 184, 238), UVec3(224, 276, 366)))
[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision=3 target_revision=4
[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit=reopen-skylight target_revision=4
[DDGI] obsolete staging promotion skipped token_serial=2 kind=Terrain terrain_revision=3 replacement_token_serial=3 replacement_terrain_revision=4
[DDGI] staging prepared token_serial=3 kind=Terrain spacing_voxels=32 active_terrain_revision=2 target_terrain_revision=4
[DDGI][LOCAL_RECOVERY] prepared geometry_revision=4 dirty_probes=48 preserved_probes=4865 minimum_epoch=4 stable_epochs=2 max_absolute_delta=0.100
[DDGI] staging promoted token_serial=3 kind=Terrain spacing_voxels=32 geometry_revision=4 radiance_revision=1 published_state=Converging published_update_epoch=5 published_source=Some(field-14)
[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=3 geometry_revision=4 radiance_revision=1 spacing_voxels=32 state=Converging update_epoch=5
[ENV_LIGHT_EDIT_CYCLE] edited probe field ready edit=reopen-skylight terrain_revision=4
[DDGI] staging prepared token_serial=4 kind=Density spacing_voxels=32 active_terrain_revision=4 target_terrain_revision=4
[ENV_LIGHT_EDIT_CYCLE] requested density rebuild terrain_revision=4 spacing_voxels=32
[DDGI] staging promoted token_serial=4 kind=Density spacing_voxels=32 geometry_revision=4 radiance_revision=1 published_state=Converging published_update_epoch=0 published_source=None
[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=4 geometry_revision=4 radiance_revision=1 spacing_voxels=32 state=Converging update_epoch=0
[ENV_LIGHT_EDIT_CYCLE] density rebuild ready terrain_revision=4
[ENV_LIGHT_EDIT_CYCLE] complete mode=reopened final_terrain_revision=4
[ENV_IRRADIANCE_CAPTURE] checkpoint target=e8 build_token_serial=4 field_serial=25 state=Converging update_epoch=8 publication=Published
[ENV_IRRADIANCE_CAPTURE] saved geometry_revision=4 radiance_revision=1 spacing_voxels=32 build_token_serial=4 field_serial=25
[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run
""".strip()


TRANSIENT = """
[ENV_IRRADIANCE_CAPTURE] checkpoint target=published build_token_serial=1 field_serial=1 state=Converging update_epoch=0 publication=Published
[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision=2
[DDGI] runtime observed visible terrain revision=3 invalidation_voxel_bound=Some((UVec3(112, 184, 238), UVec3(224, 276, 366)))
[ENV_LIGHT_EDIT_CYCLE] requested edit=close-skylight source_revision=2 target_revision=3
[DDGI] staging prepared token_serial=2 kind=Terrain spacing_voxels=32 active_terrain_revision=2 target_terrain_revision=3
[DDGI][LOCAL_RECOVERY] prepared geometry_revision=3 dirty_probes=48 preserved_probes=4865 minimum_epoch=4 stable_epochs=2 max_absolute_delta=0.100
[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit=close-skylight target_revision=3
[ENV_LIGHT_EDIT_INFLIGHT] obsolete candidate observed terrain_revision=3 active_terrain_revision=Some(2) token_serial=2 spacing_voxels=32
[DDGI] runtime observed visible terrain revision=4 invalidation_voxel_bound=Some((UVec3(112, 184, 238), UVec3(224, 276, 366))) coordinator=BuildingTerrain
[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision=3 target_revision=4
[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit=reopen-skylight target_revision=4
[DDGI] obsolete staging promotion skipped token_serial=2 kind=Terrain terrain_revision=3 replacement_token_serial=3 replacement_terrain_revision=4 coordinator=AwaitingTerrain
[DDGI] staging prepared token_serial=3 kind=Terrain spacing_voxels=32 active_terrain_revision=2 target_terrain_revision=4
[DDGI][LOCAL_RECOVERY] prepared geometry_revision=4 dirty_probes=48 preserved_probes=4865 minimum_epoch=4 stable_epochs=2 max_absolute_delta=0.100
[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] armed active_terrain_revision=Some(2) target_terrain_revision=4 staging_token_serial=Some(3) staging_stage=Rebuilding staging_progress=512/4913 coordinator=BuildingTerrain invalidation=stale-active
[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] recording active_terrain_revision=Some(2) target_terrain_revision=4 staging_token_serial=Some(3) staging_stage=Rebuilding staging_progress=1024/4913 coordinator=BuildingTerrain invalidation=stale-active
[ENV_IRRADIANCE_CAPTURE] saved geometry_revision=2 radiance_revision=1 spacing_voxels=32 build_token_serial=1 field_serial=1
[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run
""".strip()


def _swap(text: str, first: str, second: str) -> str:
    placeholder = "__SCENARIO_STREAM_SWAP__"
    return text.replace(first, placeholder, 1).replace(second, first, 1).replace(
        placeholder, second, 1
    )


def _duplicate(text: str, line: str) -> str:
    return text.replace(line, f"{line}\n{line}", 1)


class ScenarioStreamContractTests(unittest.TestCase):
    def validate(
        self,
        validation: ScenarioValidation,
        text: str,
        *,
        state: str = "",
        minimum_epoch: int = 4,
    ) -> dict[str, int]:
        with tempfile.TemporaryDirectory() as temporary:
            console = Path(temporary) / "scenario.console.log"
            console.write_text(text, encoding="utf-8")
            return validate_scenario_log(
                ValidateScenarioLog(
                    validation,
                    console,
                    32,
                    state=state,
                    minimum_epoch=minimum_epoch,
                    maximum_high_delta_epochs=0,
                )
            )

    def assert_rejected(
        self,
        validation: ScenarioValidation,
        text: str,
        *,
        state: str = "",
        minimum_epoch: int = 4,
    ) -> None:
        with self.assertRaisesRegex(ValueError, "duplicate|order|drift|identity|token"):
            self.validate(
                validation,
                text,
                state=state,
                minimum_epoch=minimum_epoch,
            )

    def test_inflight_final_rejects_order_duplicate_and_identity_mutations(self) -> None:
        self.assertEqual(
            self.validate(ScenarioValidation.INFLIGHT_FINAL, INFLIGHT_FINAL)[
                "final_revision"
            ],
            4,
        )
        request = "[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision=3 target_revision=4"
        publication = "[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit=reopen-skylight target_revision=4"
        self.assert_rejected(
            ScenarioValidation.INFLIGHT_FINAL,
            _swap(INFLIGHT_FINAL, request, publication),
        )
        self.assert_rejected(
            ScenarioValidation.INFLIGHT_FINAL,
            _duplicate(INFLIGHT_FINAL, request),
        )
        self.assert_rejected(
            ScenarioValidation.INFLIGHT_FINAL,
            INFLIGHT_FINAL.replace("replacement_token_serial=3", "replacement_token_serial=9"),
        )

    def test_local_recovery_rejects_order_duplicate_and_identity_mutations(self) -> None:
        self.assertEqual(
            self.validate(ScenarioValidation.LOCAL_RECOVERY, CLOSED)[
                "final_revision"
            ],
            3,
        )
        recovery = "[DDGI][LOCAL_RECOVERY] prepared geometry_revision=3 dirty_probes=48 preserved_probes=4865 minimum_epoch=4 stable_epochs=2 max_absolute_delta=0.100"
        promotion = "[DDGI] staging promoted token_serial=2 kind=Terrain spacing_voxels=32 geometry_revision=3 radiance_revision=1 published_state=Converging published_update_epoch=5 published_source=Some(field-7)"
        self.assert_rejected(
            ScenarioValidation.LOCAL_RECOVERY,
            _swap(CLOSED, recovery, promotion),
        )
        self.assert_rejected(
            ScenarioValidation.LOCAL_RECOVERY,
            _duplicate(CLOSED, recovery),
        )
        self.assert_rejected(
            ScenarioValidation.LOCAL_RECOVERY,
            CLOSED.replace(
                "staging promoted token_serial=2 kind=Terrain",
                "staging promoted token_serial=9 kind=Terrain",
                1,
            ),
        )

    def test_runtime_final_rejects_order_duplicate_and_identity_mutations(self) -> None:
        self.assertEqual(
            self.validate(
                ScenarioValidation.RUNTIME_FINAL,
                SEQUENTIAL_REOPENED,
                state="sequential-reopened",
                minimum_epoch=0,
            )["final_revision"],
            4,
        )
        request = "[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision=3 target_revision=4"
        publication = "[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete edit=reopen-skylight target_revision=4"
        consumer = "[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=4 geometry_revision=4 radiance_revision=1 spacing_voxels=32 state=Converging update_epoch=0"
        self.assert_rejected(
            ScenarioValidation.RUNTIME_FINAL,
            _swap(SEQUENTIAL_REOPENED, request, publication),
            state="sequential-reopened",
            minimum_epoch=0,
        )
        self.assert_rejected(
            ScenarioValidation.RUNTIME_FINAL,
            _duplicate(SEQUENTIAL_REOPENED, consumer),
            state="sequential-reopened",
            minimum_epoch=0,
        )
        self.assert_rejected(
            ScenarioValidation.RUNTIME_FINAL,
            SEQUENTIAL_REOPENED.replace(
                "active_token_serial=4 geometry_revision=4",
                "active_token_serial=99 geometry_revision=4",
                1,
            ),
            state="sequential-reopened",
            minimum_epoch=0,
        )

    def test_runtime_transient_rejects_order_duplicate_and_identity_mutations(self) -> None:
        self.assertEqual(
            self.validate(ScenarioValidation.RUNTIME_TRANSIENT, TRANSIENT)[
                "active_revision"
            ],
            2,
        )
        armed = "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] armed active_terrain_revision=Some(2) target_terrain_revision=4 staging_token_serial=Some(3) staging_stage=Rebuilding staging_progress=512/4913 coordinator=BuildingTerrain invalidation=stale-active"
        recording = "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] recording active_terrain_revision=Some(2) target_terrain_revision=4 staging_token_serial=Some(3) staging_stage=Rebuilding staging_progress=1024/4913 coordinator=BuildingTerrain invalidation=stale-active"
        self.assert_rejected(
            ScenarioValidation.RUNTIME_TRANSIENT,
            _swap(TRANSIENT, armed, recording),
        )
        self.assert_rejected(
            ScenarioValidation.RUNTIME_TRANSIENT,
            _duplicate(TRANSIENT, armed),
        )
        self.assert_rejected(
            ScenarioValidation.RUNTIME_TRANSIENT,
            TRANSIENT.replace(
                "recording active_terrain_revision=Some(2) target_terrain_revision=4 staging_token_serial=Some(3)",
                "recording active_terrain_revision=Some(2) target_terrain_revision=4 staging_token_serial=Some(9)",
            ),
        )

    def test_flora_consumer_rejects_order_duplicate_and_identity_mutations(self) -> None:
        self.assertEqual(
            self.validate(ScenarioValidation.FLORA_CONSUMER, SEQUENTIAL_REOPENED)[
                "active_token"
            ],
            4,
        )
        consumer = "[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=4 geometry_revision=4 radiance_revision=1 spacing_voxels=32 state=Converging update_epoch=0"
        flora = "[DDGI][FLORA_CONSUMER] draw_recorded active_token_serial=4 terrain_revision=4 spacing_voxels=32 instance_count=99"
        self.assert_rejected(
            ScenarioValidation.FLORA_CONSUMER,
            _swap(SEQUENTIAL_REOPENED, consumer, flora),
        )
        self.assert_rejected(
            ScenarioValidation.FLORA_CONSUMER,
            _duplicate(SEQUENTIAL_REOPENED, flora),
        )
        self.assert_rejected(
            ScenarioValidation.FLORA_CONSUMER,
            SEQUENTIAL_REOPENED.replace(
                "active_token_serial=4 geometry_revision=4",
                "active_token_serial=99 geometry_revision=4",
                1,
            ).replace(
                "active_token_serial=4 terrain_revision=4",
                "active_token_serial=99 terrain_revision=4",
                1,
            ),
        )

    def test_terrain_edit_rejects_order_duplicate_and_identity_mutations(self) -> None:
        self.assertEqual(
            self.validate(
                ScenarioValidation.TERRAIN_EDIT,
                SEQUENTIAL_REOPENED,
                state="reopened",
            )["final_revision"],
            4,
        )
        density_request = "[ENV_LIGHT_EDIT_CYCLE] requested density rebuild terrain_revision=4 spacing_voxels=32"
        density_promotion = "[DDGI] staging promoted token_serial=4 kind=Density spacing_voxels=32 geometry_revision=4 radiance_revision=1 published_state=Converging published_update_epoch=0 published_source=None"
        self.assert_rejected(
            ScenarioValidation.TERRAIN_EDIT,
            _swap(SEQUENTIAL_REOPENED, density_request, density_promotion),
            state="reopened",
        )
        self.assert_rejected(
            ScenarioValidation.TERRAIN_EDIT,
            _duplicate(SEQUENTIAL_REOPENED, density_request),
            state="reopened",
        )
        self.assert_rejected(
            ScenarioValidation.TERRAIN_EDIT,
            SEQUENTIAL_REOPENED.replace(
                "staging promoted token_serial=4 kind=Density",
                "staging promoted token_serial=99 kind=Density",
            ),
            state="reopened",
        )


if __name__ == "__main__":
    unittest.main()
