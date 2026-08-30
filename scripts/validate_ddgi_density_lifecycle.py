#!/usr/bin/env python3
"""Validate one density lifecycle as a single ordered, typed event stream."""

from __future__ import annotations

import argparse
import json
import re
from enum import Enum
from pathlib import Path
from typing import NoReturn


FIELD = re.compile(r"(?P<key>[A-Za-z][A-Za-z0-9_]*)=(?P<value>[^\s]+)")


class Phase(Enum):
    BASELINE = "baseline"
    DENSITY_MIDFLIGHT = "density-midflight"
    GEOMETRY_PREEMPTED = "geometry-preempted-density"
    GEOMETRY_PRIVATE = "geometry-e0-private"
    TERRAIN_PROMOTION = "terrain-promotion"
    TERRAIN_CONSUMERS = "terrain-consumers"
    GEOMETRY_PUBLISHED = "geometry-recovery-published"
    DENSITY_RETRY = "density-retry-midflight"
    DENSITY_PROMOTION = "density-promotion"
    DENSITY_CONSUMERS = "density-consumers"
    COMPLETE = "complete"
    SUMMARY = "summary"


ORDER = tuple(Phase)
CHECKPOINT_PHASE = {
    phase.value: phase
    for phase in (
        Phase.BASELINE,
        Phase.DENSITY_MIDFLIGHT,
        Phase.GEOMETRY_PREEMPTED,
        Phase.GEOMETRY_PRIVATE,
        Phase.GEOMETRY_PUBLISHED,
        Phase.DENSITY_RETRY,
        Phase.COMPLETE,
    )
}


class DensityLifecycleError(ValueError):
    """The console does not describe one valid density publication lifecycle."""


def _fail(message: str) -> NoReturn:
    raise DensityLifecycleError(message)


def _fields(line: str) -> dict[str, str]:
    result: dict[str, str] = {}
    for match in FIELD.finditer(line):
        key, value = match.group("key"), match.group("value").rstrip(",")
        if key in result:
            _fail(f"duplicate field {key!r} in lifecycle event")
        result[key] = value
    return result


def _required(fields: dict[str, str], *names: str) -> None:
    missing = [name for name in names if name not in fields]
    if missing:
        _fail(f"lifecycle event is missing fields: {', '.join(missing)}")


def _integer(fields: dict[str, str], name: str) -> int:
    _required(fields, name)
    value = fields[name]
    if value.startswith("Some(") and value.endswith(")"):
        value = value[5:-1]
    try:
        return int(value)
    except ValueError:
        _fail(f"lifecycle field {name} is not an integer: {fields[name]!r}")


def _literal(fields: dict[str, str], name: str, expected: str) -> None:
    _required(fields, name)
    if fields[name] != expected:
        _fail(f"lifecycle field {name} expected {expected!r}, got {fields[name]!r}")


class _Lifecycle:
    def __init__(self) -> None:
        self.next_index = 0
        self.seen: set[Phase] = set()
        self.values: dict[str, int] = {}
        self.captures: list[dict[str, str]] = []

    def event(self, phase: Phase, fields: dict[str, str]) -> None:
        if phase in self.seen:
            _fail(f"duplicate lifecycle event {phase.value}")
        expected = ORDER[self.next_index] if self.next_index < len(ORDER) else None
        if phase != expected:
            expected_label = expected.value if expected is not None else None
            _fail(
                f"lifecycle event {phase.value} is out of order; "
                f"expected {expected_label}"
            )
        self.seen.add(phase)
        self.next_index += 1
        getattr(self, phase.value.replace("-", "_"))(fields)

    def baseline(self, fields: dict[str, str]) -> None:
        self.values["baseline_field_serial"] = _integer(fields, "field_serial")
        self.values["baseline_geometry_revision"] = _integer(fields, "geometry_revision")
        self.values["radiance_revision"] = _integer(fields, "radiance_revision")
        _literal(fields, "spacing_voxels", "32")
        _literal(fields, "state", "Converging")
        _literal(fields, "update_epoch", "0")
        _literal(fields, "source_field_serial", "0")

    def density_midflight(self, fields: dict[str, str]) -> None:
        if _integer(fields, "active_field_serial") != self.values["baseline_field_serial"]:
            _fail("density midflight changed the active baseline field")
        if _integer(fields, "active_geometry_revision") != self.values["baseline_geometry_revision"]:
            _fail("density midflight changed the active geometry revision")
        _literal(fields, "active_spacing_voxels", "32")
        _literal(fields, "obsolete_density_spacing_voxels", "16")
        _literal(fields, "old_field_visible", "true")
        _literal(fields, "active_available", "true")
        self.values["active_token_serial"] = _integer(fields, "active_token_serial")
        self.values["obsolete_density_token_serial"] = _integer(
            fields, "obsolete_density_token_serial"
        )
        self.values["obsolete_density_field_serial"] = _integer(
            fields, "obsolete_density_field_serial"
        )

    def geometry_preempted_density(self, fields: dict[str, str]) -> None:
        self._same(fields, "obsolete_density_token_serial")
        self._same(fields, "obsolete_density_field_serial")
        terrain_token = _integer(fields, "terrain_token_serial")
        if terrain_token <= self.values["obsolete_density_token_serial"]:
            _fail("terrain token did not supersede the obsolete density token")
        self.values["terrain_token_serial"] = terrain_token
        self.values["geometry_revision"] = _integer(fields, "target_geometry_revision")
        _literal(fields, "terrain_spacing_voxels", "32")
        _literal(fields, "queued_density_spacing_voxels", "16")
        self._unavailable_obsolete(fields)

    def geometry_e0_private(self, fields: dict[str, str]) -> None:
        self._same(fields, "terrain_token_serial")
        self._same(fields, "generation_token_serial", expected_name="terrain_token_serial")
        self._same(fields, "obsolete_density_token_serial")
        self._same(fields, "geometry_revision")
        self._same(fields, "radiance_revision")
        source = _integer(fields, "source_field_serial")
        if source != self.values["baseline_field_serial"]:
            _fail(
                "geometry epoch-zero source field changed: "
                f"expected {self.values['baseline_field_serial']}, got {source}"
            )
        self.values["geometry_epoch_zero_field_serial"] = _integer(
            fields, "epoch_zero_field_serial"
        )
        self.values["geometry_private_current_field_serial"] = _integer(
            fields, "private_current_field_serial"
        )
        private_epoch = _integer(fields, "private_current_update_epoch")
        if private_epoch < 0:
            _fail("private geometry publication has a negative epoch")
        self.values["geometry_private_current_update_epoch"] = private_epoch
        _literal(fields, "active_spacing_voxels", "32")
        _literal(fields, "queued_density_spacing_voxels", "16")
        self._unavailable_obsolete(fields)

    def terrain_promotion(self, fields: dict[str, str]) -> None:
        self._same(fields, "token_serial", expected_name="terrain_token_serial")
        self._same(fields, "generation_token_serial", expected_name="terrain_token_serial")
        self._same(fields, "geometry_revision")
        self._same(fields, "radiance_revision")
        self._same(
            fields,
            "epoch_zero_field_serial",
            expected_name="geometry_epoch_zero_field_serial",
        )
        _literal(fields, "kind", "Terrain")
        _literal(fields, "spacing_voxels", "32")
        epoch = _integer(fields, "published_update_epoch")
        if epoch == 0:
            _fail("raw geometry epoch zero became consumer-visible")
        private_epoch = self.values["geometry_private_current_update_epoch"]
        if epoch < private_epoch:
            _fail(
                "terrain promotion regressed behind private current epoch: "
                f"private current {private_epoch}, promoted {epoch}"
            )
        published_field = _integer(fields, "published_field_serial")
        if (
            epoch == private_epoch
            and published_field
            != self.values["geometry_private_current_field_serial"]
        ):
            _fail(
                "terrain promotion changed the private current field at the same epoch"
            )
        self.values["geometry_published_update_epoch"] = epoch
        self.values["geometry_published_field_serial"] = published_field

    def terrain_consumers(self, fields: dict[str, str]) -> None:
        self._same(fields, "active_token_serial", expected_name="terrain_token_serial")
        self._same(fields, "generation_token_serial", expected_name="terrain_token_serial")
        self._same(fields, "geometry_revision")
        self._same(fields, "radiance_revision")
        self._same(
            fields,
            "epoch_zero_field_serial",
            expected_name="geometry_epoch_zero_field_serial",
        )
        self._same(
            fields,
            "published_field_serial",
            expected_name="geometry_published_field_serial",
        )
        self._same(fields, "update_epoch", expected_name="geometry_published_update_epoch")
        _literal(fields, "spacing_voxels", "32")

    def geometry_recovery_published(self, fields: dict[str, str]) -> None:
        self._same(fields, "terrain_token_serial")
        self._same(fields, "generation_token_serial", expected_name="terrain_token_serial")
        self._same(fields, "obsolete_density_token_serial")
        self._same(fields, "geometry_revision")
        self._same(fields, "radiance_revision")
        self._same(
            fields,
            "epoch_zero_field_serial",
            expected_name="geometry_epoch_zero_field_serial",
        )
        self._same(
            fields,
            "published_update_epoch",
            expected_name="geometry_published_update_epoch",
        )
        self._same(
            fields,
            "published_field_serial",
            expected_name="geometry_published_field_serial",
        )
        _literal(fields, "same_generation", "true")
        _literal(fields, "active_spacing_voxels", "32")
        _literal(fields, "queued_density_spacing_voxels", "16")
        self._unavailable_obsolete(fields)

    def density_retry_midflight(self, fields: dict[str, str]) -> None:
        self._same(fields, "active_token_serial", expected_name="terrain_token_serial")
        self._same(
            fields,
            "active_field_serial",
            expected_name="geometry_published_field_serial",
        )
        self._same(fields, "active_geometry_revision", expected_name="geometry_revision")
        self._same(fields, "active_radiance_revision", expected_name="radiance_revision")
        self._same(fields, "density_radiance_revision", expected_name="radiance_revision")
        _literal(fields, "active_spacing_voxels", "32")
        _literal(fields, "density_spacing_voxels", "16")
        _literal(fields, "old_field_visible", "true")
        _literal(fields, "active_available", "true")
        density_token = _integer(fields, "density_token_serial")
        if density_token <= self.values["terrain_token_serial"]:
            _fail("retried density token did not supersede the terrain token")
        self.values["density_token_serial"] = density_token
        self.values["density_field_serial"] = _integer(fields, "density_field_serial")

    def density_promotion(self, fields: dict[str, str]) -> None:
        self._same(fields, "token_serial", expected_name="density_token_serial")
        self._same(fields, "generation_token_serial", expected_name="density_token_serial")
        self._same(fields, "geometry_revision")
        self._same(fields, "radiance_revision")
        self._same(
            fields, "epoch_zero_field_serial", expected_name="density_field_serial"
        )
        self._same(fields, "published_field_serial", expected_name="density_field_serial")
        _literal(fields, "kind", "Density")
        _literal(fields, "spacing_voxels", "16")
        _literal(fields, "published_update_epoch", "0")

    def density_consumers(self, fields: dict[str, str]) -> None:
        self._same(fields, "active_token_serial", expected_name="density_token_serial")
        self._same(fields, "generation_token_serial", expected_name="density_token_serial")
        self._same(fields, "geometry_revision")
        self._same(fields, "radiance_revision")
        self._same(
            fields, "epoch_zero_field_serial", expected_name="density_field_serial"
        )
        self._same(fields, "published_field_serial", expected_name="density_field_serial")
        _literal(fields, "spacing_voxels", "16")
        _literal(fields, "update_epoch", "0")

    def complete(self, fields: dict[str, str]) -> None:
        self._same(fields, "field_serial", expected_name="density_field_serial")
        self._same(fields, "geometry_revision")
        self._same(fields, "radiance_revision")
        _literal(fields, "spacing_voxels", "16")
        _literal(fields, "state", "Converging")
        _literal(fields, "update_epoch", "0")
        self.values["field_serial"] = _integer(fields, "field_serial")
        _literal(fields, "source_field_serial", "0")
        _literal(fields, "source_state", "none")
        self.values["source_field_serial"] = 0

    def summary(self, fields: dict[str, str]) -> None:
        self._same(fields, "obsolete_density_token_serial")
        self._same(fields, "terrain_token_serial")
        self._same(fields, "density_token_serial")
        self._same(fields, "geometry_revision")
        self._same(fields, "radiance_revision")
        _literal(fields, "obsolete_density_consumer_visible", "false")
        _literal(fields, "first_consumer_visible_16_epoch", "0")
        _literal(fields, "spacing_voxels", "16")

    def promotion(self, fields: dict[str, str]) -> None:
        token = _integer(fields, "token_serial")
        if token == self.values.get("obsolete_density_token_serial"):
            _fail("obsolete density token was promoted")
        if token == self.values.get("terrain_token_serial"):
            self.event(Phase.TERRAIN_PROMOTION, fields)
        elif token == self.values.get("density_token_serial"):
            self.event(Phase.DENSITY_PROMOTION, fields)
        elif Phase.BASELINE in self.seen:
            _fail(f"mixed-log promotion token {token}")

    def consumers(self, fields: dict[str, str]) -> None:
        _required(fields, "consumer_set", "active_token_serial")
        token = _integer(fields, "active_token_serial")
        if token == self.values.get("obsolete_density_token_serial"):
            _fail("obsolete density token became consumer-active")
        if token == self.values.get("terrain_token_serial"):
            self.event(Phase.TERRAIN_CONSUMERS, fields)
        elif token == self.values.get("density_token_serial"):
            self.event(Phase.DENSITY_CONSUMERS, fields)
        elif Phase.BASELINE in self.seen:
            _fail(f"mixed-log consumer token {token}")

    def capture(self, fields: dict[str, str]) -> None:
        _required(
            fields,
            "build_token_serial",
            "field_serial",
            "generation_token_serial",
            "epoch_zero_field_serial",
            "source_field_serial",
            "geometry_revision",
            "radiance_revision",
            "spacing_voxels",
            "state",
            "update_epoch",
            "publication",
        )
        self.captures.append(fields)

    def finish(self) -> dict[str, int]:
        if self.next_index != len(ORDER):
            _fail(
                f"incomplete density lifecycle; expected {ORDER[self.next_index].value}"
            )
        expected = {
            self.values["active_token_serial"]: (
                "baseline",
                self.values["baseline_field_serial"],
                0,
                self.values["baseline_geometry_revision"],
                32,
            ),
            self.values["obsolete_density_token_serial"]: (
                "obsolete density",
                self.values["obsolete_density_field_serial"],
                0,
                self.values["baseline_geometry_revision"],
                16,
            ),
            self.values["terrain_token_serial"]: (
                "terrain",
                self.values["geometry_epoch_zero_field_serial"],
                self.values["baseline_field_serial"],
                self.values["geometry_revision"],
                32,
            ),
            self.values["density_token_serial"]: (
                "density",
                self.values["density_field_serial"],
                0,
                self.values["geometry_revision"],
                16,
            ),
        }
        seen: set[int] = set()
        for capture in self.captures:
            token = _integer(capture, "build_token_serial")
            generation = _integer(capture, "generation_token_serial")
            if token != generation:
                _fail(
                    f"capture build token {token} differs from generation token {generation}"
                )
            if token not in expected:
                _fail(f"unknown capture generation token {token}")
            if token in seen:
                _fail(f"duplicate capture for generation token {token}")
            seen.add(token)
            label, root, source, geometry, spacing = expected[token]
            for field_name in ("epoch_zero_field_serial", "field_serial"):
                actual = _integer(capture, field_name)
                if actual != root:
                    _fail(
                        f"{label} capture {field_name} expected {root}, got {actual}"
                    )
            for field_name, value in (
                ("source_field_serial", source),
                ("geometry_revision", geometry),
                ("radiance_revision", self.values["radiance_revision"]),
                ("spacing_voxels", spacing),
            ):
                actual = _integer(capture, field_name)
                if actual != value:
                    _fail(
                        f"{label} capture {field_name} expected {value}, got {actual}"
                    )
            _literal(capture, "state", "Converging")
            _literal(capture, "update_epoch", "0")
            _literal(capture, "publication", "Published")
        missing = set(expected) - seen
        if missing:
            _fail(f"missing capture generations: {sorted(missing)}")
        self.values["build_token_serial"] = self.values["density_token_serial"]
        return dict(self.values)

    def _same(
        self,
        fields: dict[str, str],
        field_name: str,
        *,
        expected_name: str | None = None,
    ) -> None:
        expected_name = expected_name or field_name
        actual = _integer(fields, field_name)
        expected = self.values[expected_name]
        if actual != expected:
            label = expected_name.replace("_serial", "").replace("_", " ")
            _fail(f"{label} changed: expected {expected}, got {actual}")

    @staticmethod
    def _unavailable_obsolete(fields: dict[str, str]) -> None:
        _literal(fields, "obsolete_density_consumer_visible", "false")
        _literal(fields, "active_available", "true")


def validate_density_lifecycle(console: Path) -> dict[str, int]:
    lifecycle = _Lifecycle()
    for line in console.read_text(errors="replace").splitlines():
        if "[ENV_IRRADIANCE_CAPTURE] checkpoint target=e0" in line:
            lifecycle.capture(_fields(line))
            continue
        if "[DDGI] staging promoted " in line:
            lifecycle.promotion(_fields(line.split(" published_source=", 1)[0]))
            continue
        if "[DDGI][CONSUMERS] consumer_set=" in line:
            lifecycle.consumers(_fields(line.split(" source=", 1)[0]))
            continue
        if "[DDGI_ACCEPT][DENSITY]" not in line:
            continue
        fields = _fields(line)
        checkpoint = fields.get("checkpoint")
        if checkpoint in CHECKPOINT_PHASE:
            lifecycle.event(CHECKPOINT_PHASE[checkpoint], fields)
        elif "[DDGI_ACCEPT][DENSITY] complete " in line:
            lifecycle.event(Phase.SUMMARY, fields)
    return lifecycle.finish()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("console", type=Path)
    arguments = parser.parse_args()
    try:
        result = validate_density_lifecycle(arguments.console)
    except (OSError, DensityLifecycleError) as error:
        parser.error(str(error))
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
