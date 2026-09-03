from __future__ import annotations

import tempfile
import unittest
from dataclasses import dataclass
from pathlib import Path

from scripts.ddgi_evidence.model import ScenarioValidation, ValidateScenarioLog
from scripts.ddgi_evidence.validation import DensityLifecycleError, validate_scenario_log


@dataclass(frozen=True)
class Event:
    name: str
    payload: str


def event(name: str, payload: str) -> Event:
    return Event(name, payload)


def capture(
    name: str,
    *,
    token: int,
    root: int,
    source: int,
    geometry: int,
    spacing: int,
) -> Event:
    return event(
        name,
        "[ENV_IRRADIANCE_CAPTURE] checkpoint "
        f"target=e0 build_token_serial={token} generation_token_serial={token} "
        f"epoch_zero_field_serial={root} field_serial={root} "
        f"source_field_serial={source} geometry_revision={geometry} "
        f"radiance_revision=1 spacing_voxels={spacing} state=Converging "
        "update_epoch=0 publication=Published",
    )


def obsolete_density_capture() -> Event:
    return capture(
        "obsolete_density_capture",
        token=2,
        root=3,
        source=0,
        geometry=2,
        spacing=16,
    )


def valid_events() -> list[Event]:
    return [
        capture(
            "baseline_capture",
            token=1,
            root=1,
            source=0,
            geometry=2,
            spacing=32,
        ),
        event(
            "baseline",
            "[DDGI_ACCEPT][DENSITY] checkpoint=baseline field_serial=1 geometry_revision=2 radiance_revision=1 spacing_voxels=32 state=Converging update_epoch=0 source_field_serial=0 source_state=none",
        ),
        event(
            "density_midflight",
            "[DDGI_ACCEPT][DENSITY] checkpoint=density-midflight active_token_serial=1 active_field_serial=1 active_geometry_revision=2 active_spacing_voxels=32 obsolete_density_token_serial=2 obsolete_density_field_serial=3 obsolete_density_spacing_voxels=16 old_field_visible=true active_available=true",
        ),
        event(
            "geometry_preempted",
            "[DDGI_ACCEPT][DENSITY] checkpoint=geometry-preempted-density obsolete_density_token_serial=2 obsolete_density_field_serial=3 terrain_token_serial=3 target_geometry_revision=3 terrain_spacing_voxels=32 queued_density_spacing_voxels=16 obsolete_density_consumer_visible=false active_available=true",
        ),
        capture(
            "terrain_capture",
            token=3,
            root=5,
            source=1,
            geometry=3,
            spacing=32,
        ),
        event(
            "geometry_private",
            "[DDGI_ACCEPT][DENSITY] checkpoint=geometry-e0-private terrain_token_serial=3 generation_token_serial=3 obsolete_density_token_serial=2 geometry_revision=3 radiance_revision=1 epoch_zero_field_serial=5 source_field_serial=1 private_current_field_serial=5 private_current_update_epoch=0 active_spacing_voxels=32 queued_density_spacing_voxels=16 obsolete_density_consumer_visible=false active_available=true",
        ),
        event(
            "terrain_promotion",
            "[DDGI] staging promoted token_serial=3 generation_token_serial=3 kind=Terrain spacing_voxels=32 geometry_revision=3 radiance_revision=1 epoch_zero_field_serial=5 published_field_serial=10 published_update_epoch=5 published_source=Some(DdgiFieldKey { geometry_revision: 3 }) building=Some(DdgiFieldIdentity { spacing_voxels: 32 })",
        ),
        event(
            "terrain_consumers",
            "[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=3 generation_token_serial=3 geometry_revision=3 radiance_revision=1 spacing_voxels=32 epoch_zero_field_serial=5 published_field_serial=10 update_epoch=5 source=Some(DdgiFieldKey { geometry_revision: 3 })",
        ),
        event(
            "geometry_published",
            "[DDGI_ACCEPT][DENSITY] checkpoint=geometry-recovery-published terrain_token_serial=3 generation_token_serial=3 obsolete_density_token_serial=2 geometry_revision=3 radiance_revision=1 epoch_zero_field_serial=5 published_field_serial=10 published_update_epoch=5 same_generation=true active_spacing_voxels=32 queued_density_spacing_voxels=16 obsolete_density_consumer_visible=false active_available=true",
        ),
        event(
            "density_retry",
            "[DDGI_ACCEPT][DENSITY] checkpoint=density-retry-midflight active_token_serial=3 active_field_serial=10 active_geometry_revision=3 active_radiance_revision=1 active_spacing_voxels=32 density_token_serial=4 density_field_serial=12 density_radiance_revision=1 density_spacing_voxels=16 progress=512/35937 old_field_visible=true active_available=true",
        ),
        capture(
            "density_capture",
            token=4,
            root=12,
            source=0,
            geometry=3,
            spacing=16,
        ),
        event(
            "density_promotion",
            "[DDGI] staging promoted token_serial=4 generation_token_serial=4 kind=Density spacing_voxels=16 geometry_revision=3 radiance_revision=1 epoch_zero_field_serial=12 published_field_serial=12 published_update_epoch=0",
        ),
        event(
            "density_consumers",
            "[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster active_token_serial=4 generation_token_serial=4 geometry_revision=3 radiance_revision=1 spacing_voxels=16 epoch_zero_field_serial=12 published_field_serial=12 update_epoch=0",
        ),
        event(
            "complete",
            "[DDGI_ACCEPT][DENSITY] checkpoint=complete field_serial=12 geometry_revision=3 radiance_revision=1 spacing_voxels=16 state=Converging update_epoch=0 source_field_serial=0 source_state=none",
        ),
        event(
            "summary",
            "[DDGI_ACCEPT][DENSITY] complete obsolete_density_token_serial=2 terrain_token_serial=3 density_token_serial=4 obsolete_density_consumer_visible=false first_consumer_visible_16_epoch=0 geometry_revision=3 radiance_revision=1 spacing_voxels=16",
        ),
    ]


def only_index(events: list[Event], name: str) -> int:
    matches = [index for index, candidate in enumerate(events) if candidate.name == name]
    if len(matches) != 1:
        raise AssertionError(f"expected exactly one event {name!r}, found {len(matches)}")
    return matches[0]


def mutate(events: list[Event], name: str, old: str, new: str) -> None:
    index = only_index(events, name)
    candidate = events[index]
    occurrences = candidate.payload.count(old)
    if occurrences != 1:
        raise AssertionError(
            f"expected exactly one {old!r} in event {name!r}, found {occurrences}"
        )
    events[index] = Event(name, candidate.payload.replace(old, new, 1))


def derived(
    events: list[Event],
    source: str,
    name: str,
    *replacements: tuple[str, str],
) -> Event:
    candidate = events[only_index(events, source)]
    scratch = [Event(name, candidate.payload)]
    for old, new in replacements:
        mutate(scratch, name, old, new)
    return scratch[0]


def insert_before(events: list[Event], anchor: str, candidate: Event) -> None:
    if any(item.name == candidate.name for item in events):
        raise AssertionError(f"event {candidate.name!r} already exists")
    events.insert(only_index(events, anchor), candidate)


def move_before(events: list[Event], name: str, anchor: str) -> None:
    candidate = events.pop(only_index(events, name))
    events.insert(only_index(events, anchor), candidate)


def move_after_summary(events: list[Event], *names: str) -> None:
    moved = [events.pop(only_index(events, name)) for name in names]
    events.extend(moved)


class ValidateDdgiDensityLifecycleTests(unittest.TestCase):
    def validate(self, events: list[Event]) -> dict[str, int]:
        with tempfile.TemporaryDirectory() as directory:
            console = Path(directory) / "console.log"
            console.write_text(
                "\n".join(
                    f"[00:00:00 INFO re_flora] {candidate.payload}"
                    for candidate in events
                )
                + "\n"
            )
            return validate_scenario_log(
                ValidateScenarioLog(
                    ScenarioValidation.DENSITY_STREAM,
                    console,
                    32,
                )
            )

    def test_accepts_one_strict_owner_lineage(self) -> None:
        result = self.validate(valid_events())
        self.assertEqual(result["obsolete_density_token_serial"], 2)
        self.assertEqual(result["terrain_token_serial"], 3)
        self.assertEqual(result["density_token_serial"], 4)
        self.assertEqual(result["geometry_epoch_zero_field_serial"], 5)
        self.assertEqual(result["geometry_published_update_epoch"], 5)
        self.assertEqual(result["field_serial"], 12)
        self.assertEqual(result["build_token_serial"], 4)

    def test_rejects_shuffled_private_and_preemption_checkpoints(self) -> None:
        events = valid_events()
        move_before(events, "geometry_private", "geometry_preempted")
        with self.assertRaisesRegex(DensityLifecycleError, "out of order"):
            self.validate(events)

    def test_rejects_duplicate_checkpoint(self) -> None:
        events = valid_events()
        duplicate = derived(events, "geometry_private", "duplicate_geometry_private")
        insert_before(events, "terrain_promotion", duplicate)
        with self.assertRaisesRegex(DensityLifecycleError, "duplicate"):
            self.validate(events)

    def test_rejects_mixed_log_identity(self) -> None:
        events = valid_events()
        mutate(
            events,
            "geometry_published",
            "terrain_token_serial=3",
            "terrain_token_serial=33",
        )
        with self.assertRaisesRegex(DensityLifecycleError, "terrain token"):
            self.validate(events)

    def test_rejects_obsolete_token_promotion_even_between_checkpoints(self) -> None:
        events = valid_events()
        promotion = event(
            "obsolete_density_promotion",
            "[DDGI] staging promoted token_serial=2 kind=Density spacing_voxels=16 geometry_revision=2 published_update_epoch=0",
        )
        insert_before(events, "terrain_capture", promotion)
        with self.assertRaisesRegex(DensityLifecycleError, "obsolete density token"):
            self.validate(events)

    def test_rejects_future_obsolete_promotion_before_its_identity_is_declared(self) -> None:
        events = valid_events()
        promotion = derived(
            events,
            "density_promotion",
            "future_obsolete_promotion",
            (
                "staging promoted token_serial=4",
                "staging promoted token_serial=2",
            ),
        )
        insert_before(events, "density_midflight", promotion)
        with self.assertRaisesRegex(DensityLifecycleError, "promotion"):
            self.validate(events)

    def test_rejects_mixed_consumer_before_candidate_identity_is_declared(self) -> None:
        events = valid_events()
        consumer = derived(
            events,
            "density_consumers",
            "future_mixed_consumer",
            ("active_token_serial=4", "active_token_serial=99"),
        )
        insert_before(events, "density_midflight", consumer)
        with self.assertRaisesRegex(DensityLifecycleError, "consumer"):
            self.validate(events)

    def test_rejects_coordinated_geometry_field_rewrite_without_owner_markers(self) -> None:
        events = valid_events()
        mutate(
            events,
            "geometry_published",
            "published_field_serial=10",
            "published_field_serial=999",
        )
        mutate(
            events,
            "density_retry",
            "active_field_serial=10",
            "active_field_serial=999",
        )
        with self.assertRaisesRegex(DensityLifecycleError, "field"):
            self.validate(events)

    def test_rejects_promotion_and_consumer_radiance_rewrite(self) -> None:
        events = valid_events()
        mutate(
            events,
            "terrain_promotion",
            "radiance_revision=1",
            "radiance_revision=99",
        )
        mutate(
            events,
            "terrain_consumers",
            "radiance_revision=1",
            "radiance_revision=99",
        )
        with self.assertRaisesRegex(DensityLifecycleError, "radiance"):
            self.validate(events)

    def test_every_later_checkpoint_is_bound_to_baseline_radiance(self) -> None:
        mutations = (
            ("baseline_capture", "radiance_revision=1", "radiance_revision=99"),
            ("terrain_capture", "radiance_revision=1", "radiance_revision=99"),
            ("geometry_private", "radiance_revision=1", "radiance_revision=99"),
            ("geometry_published", "radiance_revision=1", "radiance_revision=99"),
            ("density_retry", "active_radiance_revision=1", "active_radiance_revision=99"),
            ("density_retry", "density_radiance_revision=1", "density_radiance_revision=99"),
            ("density_capture", "radiance_revision=1", "radiance_revision=99"),
            ("density_promotion", "radiance_revision=1", "radiance_revision=99"),
            ("density_consumers", "radiance_revision=1", "radiance_revision=99"),
            ("complete", "radiance_revision=1", "radiance_revision=99"),
            ("summary", "radiance_revision=1", "radiance_revision=99"),
        )
        for name, old, new in mutations:
            with self.subTest(name=name, identity=old):
                events = valid_events()
                mutate(events, name, old, new)
                with self.assertRaisesRegex(DensityLifecycleError, "radiance"):
                    self.validate(events)

    def test_rejects_final_density_epoch_with_a_source(self) -> None:
        events = valid_events()
        mutate(
            events,
            "complete",
            "source_field_serial=0",
            "source_field_serial=10",
        )
        with self.assertRaisesRegex(DensityLifecycleError, "source"):
            self.validate(events)

    def test_rejects_geometry_epoch_zero_from_a_nonbaseline_history_source(self) -> None:
        events = valid_events()
        mutate(
            events,
            "geometry_private",
            "source_field_serial=1",
            "source_field_serial=999",
        )
        with self.assertRaisesRegex(DensityLifecycleError, "source"):
            self.validate(events)

    def test_rejects_private_current_epoch_regression_at_promotion(self) -> None:
        events = valid_events()
        mutate(
            events,
            "geometry_private",
            "private_current_update_epoch=0",
            "private_current_update_epoch=6",
        )
        with self.assertRaisesRegex(DensityLifecycleError, "private current"):
            self.validate(events)

    def test_same_epoch_promotion_must_publish_the_private_current_field(self) -> None:
        events = valid_events()
        mutate(
            events,
            "geometry_private",
            "private_current_field_serial=5",
            "private_current_field_serial=999",
        )
        mutate(
            events,
            "geometry_private",
            "private_current_update_epoch=0",
            "private_current_update_epoch=5",
        )
        with self.assertRaisesRegex(DensityLifecycleError, "private current"):
            self.validate(events)

    def test_rejects_missing_private_current_field(self) -> None:
        events = valid_events()
        mutate(events, "geometry_private", " private_current_field_serial=5", "")
        with self.assertRaisesRegex(DensityLifecycleError, "missing fields"):
            self.validate(events)

    def test_rejects_unknown_complete_epoch_zero_capture_after_baseline(self) -> None:
        events = valid_events()
        unknown = derived(
            events,
            "density_capture",
            "unknown_capture",
            ("build_token_serial=4", "build_token_serial=99"),
            ("generation_token_serial=4", "generation_token_serial=99"),
            ("epoch_zero_field_serial=12", "epoch_zero_field_serial=99"),
            ("field_serial=12", "field_serial=99"),
        )
        insert_before(events, "density_midflight", unknown)
        with self.assertRaisesRegex(DensityLifecycleError, "unknown capture"):
            self.validate(events)

    def test_rejects_duplicate_terrain_epoch_zero_capture(self) -> None:
        events = valid_events()
        duplicate = derived(events, "terrain_capture", "duplicate_terrain_capture")
        insert_before(events, "geometry_private", duplicate)
        with self.assertRaisesRegex(DensityLifecycleError, "duplicate capture"):
            self.validate(events)

    def test_rejects_epoch_zero_capture_for_preempted_density(self) -> None:
        events = valid_events()
        insert_before(events, "geometry_preempted", obsolete_density_capture())
        with self.assertRaisesRegex(DensityLifecycleError, "obsolete density capture"):
            self.validate(events)

    def test_rejects_nonmonotonic_generation_tokens_before_capture_matching(self) -> None:
        events = valid_events()
        mutate(
            events,
            "baseline_capture",
            "build_token_serial=1",
            "build_token_serial=5",
        )
        mutate(
            events,
            "baseline_capture",
            "generation_token_serial=1",
            "generation_token_serial=5",
        )
        mutate(
            events,
            "density_midflight",
            "active_token_serial=1",
            "active_token_serial=5",
        )
        insert_before(events, "geometry_preempted", obsolete_density_capture())
        with self.assertRaisesRegex(DensityLifecycleError, "strictly increasing"):
            self.validate(events)

    def test_private_epoch_zero_current_field_must_equal_its_root(self) -> None:
        events = valid_events()
        mutate(
            events,
            "geometry_private",
            "private_current_field_serial=5",
            "private_current_field_serial=999",
        )
        with self.assertRaisesRegex(DensityLifecycleError, "epoch-zero root"):
            self.validate(events)

    def test_rejects_capture_events_outside_their_arrival_windows(self) -> None:
        cases = (
            ("all", ("baseline_capture", "terrain_capture", "density_capture")),
            ("baseline", ("baseline_capture",)),
            ("terrain", ("terrain_capture",)),
            ("density", ("density_capture",)),
        )
        for label, names in cases:
            with self.subTest(label=label):
                events = valid_events()
                move_after_summary(events, *names)
                with self.assertRaisesRegex(DensityLifecycleError, "capture window"):
                    self.validate(events)

    def test_rejects_density_capture_before_retry_declares_its_token(self) -> None:
        events = valid_events()
        move_before(events, "density_capture", "density_retry")
        with self.assertRaisesRegex(DensityLifecycleError, "capture window"):
            self.validate(events)

    def test_rejects_nonexact_epoch_zero_capture_targets(self) -> None:
        for target in ("e00", "e0x", "e0-forged"):
            with self.subTest(target=target):
                events = valid_events()
                mutate(events, "density_capture", "target=e0", f"target={target}")
                with self.assertRaisesRegex(DensityLifecycleError, "target"):
                    self.validate(events)


if __name__ == "__main__":
    unittest.main()
