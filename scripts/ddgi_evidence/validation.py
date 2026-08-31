from __future__ import annotations

import json
import re
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import NoReturn

try:
    from runtime_log_diagnostics import first_fatal_diagnostic
except ModuleNotFoundError:  # package import from the repository root
    from scripts.runtime_log_diagnostics import first_fatal_diagnostic

from .model import ScenarioValidation, ValidateRadianceLifecycle, ValidateScenarioLog


FIELD = re.compile(r"(?P<key>[A-Za-z][A-Za-z0-9_]*)=(?P<value>[^\s]+)")
LOG_TIME = r"(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d\.\d{3}"


def _production_log_line(level: str, module: str, payload: str) -> re.Pattern[str]:
    return re.compile(
        rf"^\[{LOG_TIME} {level} {re.escape(module)}\] {payload}$", re.MULTILINE
    )


RUN_LOG_MARKER = _production_log_line(
    "INFO", "re_flora", r"\[RUN_LOG\] path=(?P<path>.+?)"
)
PUBLICATION = _production_log_line(
    "INFO",
    "re_flora::app::core::environment_lighting_test_scene",
    r"\[ENV_LIGHT_TEST\] static terrain ready .*?terrain_revision=(\d+).*",
)
INITIALIZATION = _production_log_line(
    "INFO", "re_flora::tracer", r"\[DDGI\] initialization requested terrain_revision=(\d+).*"
)
VERIFICATION = _production_log_line(
    "INFO",
    "re_flora::app::core::environment_lighting_test_scene",
    r"\[ENV_LIGHT_TEST\] first DDGI build verified .*?geometry_revision=(\d+) "
    r"visible_terrain_publication_revision=(\d+).*",
)
CAPTURE_SAVED = _production_log_line(
    "INFO",
    "re_flora::app::core::environment_irradiance_capture",
    r"\[ENV_IRRADIANCE_CAPTURE\] saved\b.*",
)
CAPTURE_COMPLETE = _production_log_line(
    "INFO",
    "re_flora::app::core",
    r"\[ENV_IRRADIANCE_CAPTURE\] complete; exiting one-shot capture run",
)


def _exactly_one(pattern: re.Pattern[str], text: str, label: str) -> re.Match[str]:
    matches = list(pattern.finditer(text))
    if len(matches) != 1:
        raise ValueError(f"expected exactly one {label}, found {len(matches)}")
    return matches[0]


def validate_process_evidence(
    console_path: Path, *, require_test_scene_startup: bool
) -> Path:
    console = console_path.read_text(encoding="utf-8", errors="replace")
    marker = _exactly_one(RUN_LOG_MARKER, console, "process-bound [RUN_LOG] marker")
    run_log_path = Path(marker.group("path"))
    if not run_log_path.is_absolute():
        raise ValueError(f"process-bound run log path is not absolute: {run_log_path}")
    try:
        canonical_run_log = run_log_path.resolve(strict=True)
    except OSError as error:
        raise ValueError(
            f"process-bound run log is unavailable: {run_log_path}: {error}"
        ) from error
    if canonical_run_log != run_log_path:
        raise ValueError(
            "process-bound run log marker is not canonical: "
            f"{run_log_path} != {canonical_run_log}"
        )
    run_log = canonical_run_log.read_text(encoding="utf-8", errors="replace")
    log_marker = _exactly_one(RUN_LOG_MARKER, run_log, "run-log [RUN_LOG] marker")
    if Path(log_marker.group("path")) != canonical_run_log:
        raise ValueError("console and run-log [RUN_LOG] markers disagree")
    for label, text in (("console", console), ("run log", run_log)):
        diagnostic = first_fatal_diagnostic(text)
        if diagnostic is not None:
            raise ValueError(
                f"{label} contains fatal or validation diagnostic: "
                f"{diagnostic.group(0).strip()}"
            )
        saved = _exactly_one(CAPTURE_SAVED, text, f"{label} capture saved event")
        complete = _exactly_one(
            CAPTURE_COMPLETE, text, f"{label} capture completion event"
        )
        if saved.start() >= complete.start():
            raise ValueError(f"{label} capture completion precedes capture save")
    if require_test_scene_startup:
        for label, text in (("console", console), ("run log", run_log)):
            publication = _exactly_one(
                PUBLICATION, text, f"{label} test-scene terrain publication"
            )
            initialization = _exactly_one(
                INITIALIZATION, text, f"{label} first DDGI initialization"
            )
            verification = _exactly_one(
                VERIFICATION, text, f"{label} first DDGI build verification"
            )
            revisions = (
                int(publication.group(1)),
                int(initialization.group(1)),
                int(verification.group(1)),
                int(verification.group(2)),
            )
            if len(set(revisions)) != 1:
                raise ValueError(
                    f"{label} test-scene publication and first DDGI build revisions differ: "
                    f"publication={revisions[0]} initialization={revisions[1]} "
                    f"build={revisions[2]} verified_publication={revisions[3]}"
                )
            if not (publication.start() < initialization.start() < verification.start()):
                raise ValueError(
                    f"{label} test-scene Visible Terrain Publication must precede first "
                    "DDGI initialization and typed build verification"
                )
    return canonical_run_log


def _fields(line: str) -> dict[str, str]:
    result: dict[str, str] = {}
    for match in FIELD.finditer(line):
        name, value = match.group("key"), match.group("value").rstrip(",")
        if name in result:
            raise ValueError(f"duplicate field {name!r} in evidence event")
        result[name] = value
    return result


def _required(fields: dict[str, str], *names: str) -> None:
    missing = [name for name in names if name not in fields]
    if missing:
        raise ValueError(f"evidence event is missing fields: {', '.join(missing)}")


def _integer(fields: dict[str, str], name: str) -> int:
    _required(fields, name)
    value = fields[name]
    if value.startswith("Some(") and value.endswith(")"):
        value = value[5:-1]
    try:
        return int(value)
    except ValueError as error:
        raise ValueError(f"evidence field {name} is not an integer: {fields[name]!r}") from error


def _literal(fields: dict[str, str], name: str, expected: str) -> None:
    _required(fields, name)
    if fields[name] != expected:
        raise ValueError(
            f"evidence field {name} expected {expected!r}, got {fields[name]!r}"
        )


class RadianceLifecycleError(ValueError):
    """The log is not one ordered radiance publication lifecycle."""


class _RadiancePhase(Enum):
    TERMINAL = "r1-terminal"
    BASELINE_ARMED = "baseline"
    BASELINE_CAPTURE = "baseline-capture"
    MUTATION_R2 = "mutation-r2"
    R2_ARMED = "r2-next-frame"
    R2_CAPTURE = "r2-capture"
    R2_MIDFLIGHT = "r2-midflight"
    MUTATION_R3 = "mutation-r3"
    R3_OBSERVED = "r3-observed"
    MUTATION_R4 = "mutation-r4"
    R4_ARMED = "r4-next-frame"
    R4_CAPTURE = "r4-capture"
    R2_PUBLICATION = "r2-publication"
    R4_MIDFLIGHT = "r4-midflight"
    R4_PUBLICATION = "r4-publication"
    COMPLETE_FIELD = "complete-field"
    COMPLETE_SUMMARY = "complete-summary"
    FINAL_ARMED = "final"
    FINAL_CAPTURE = "final-capture"


RADIANCE_ORDER = tuple(_RadiancePhase)


@dataclass
class _RadianceStream:
    spacing_voxels: int
    index: int = 0

    def __post_init__(self) -> None:
        self.values: dict[str, int] = {}
        self.captures = 0

    def event(self, phase: _RadiancePhase, fields: dict[str, str]) -> None:
        if self.index >= len(RADIANCE_ORDER) or RADIANCE_ORDER[self.index] is not phase:
            expected = (
                RADIANCE_ORDER[self.index].value
                if self.index < len(RADIANCE_ORDER)
                else "end-of-lifecycle"
            )
            raise RadianceLifecycleError(
                f"radiance event {phase.value} is duplicate or out of order; expected {expected}"
            )
        self.index += 1
        getattr(self, phase.name.lower())(fields)

    def terminal(self, fields: dict[str, str]) -> None:
        self.values["r1_field"] = _integer(fields, "field_serial")
        self.values["geometry_revision"] = _integer(fields, "geometry_revision")
        self.values["r1_revision"] = _integer(fields, "radiance_revision")
        if _integer(fields, "spacing_voxels") != self.spacing_voxels:
            raise RadianceLifecycleError("terminal spacing drift")

    def baseline_armed(self, fields: dict[str, str]) -> None:
        self._capture_request(fields, revision=self.values["r1_revision"], building=0)

    def baseline_capture(self, fields: dict[str, str]) -> None:
        self._resident_capture(fields, self.values["r1_field"], self.values["r1_revision"])

    def mutation_r2(self, fields: dict[str, str]) -> None:
        self.values["r2_revision"] = self.values["r1_revision"] + 1
        self._mutation(fields, self.values["r2_revision"])

    def r2_armed(self, fields: dict[str, str]) -> None:
        self.values["r2_field"] = _integer(fields, "building_field_serial")
        if self.values["r2_field"] != self.values["r1_field"] + 1:
            raise RadianceLifecycleError("r2 field serial drift")
        self._capture_request(
            fields,
            revision=self.values["r2_revision"],
            building=self.values["r2_field"],
            active_revision=self.values["r1_revision"],
        )
        self._next_frame(fields)

    def r2_capture(self, fields: dict[str, str]) -> None:
        self._resident_capture(fields, self.values["r1_field"], self.values["r1_revision"])

    def r2_midflight(self, fields: dict[str, str]) -> None:
        self._same(fields, "active_field_serial", "r1_field")
        self._same(fields, "active_radiance_revision", "r1_revision")
        self._same(fields, "building_field_serial", "r2_field")
        self._same(fields, "building_radiance_revision", "r2_revision")
        self._same(fields, "source_field_serial", "r1_field")
        _literal(fields, "building_update_epoch", "0")
        _literal(fields, "old_field_visible", "true")

    def mutation_r3(self, fields: dict[str, str]) -> None:
        self.values["r3_revision"] = self.values["r2_revision"] + 1
        self._mutation(fields, self.values["r3_revision"])

    def r3_observed(self, fields: dict[str, str]) -> None:
        self._same(fields, "latest_radiance_revision", "r3_revision")
        self._same(fields, "inflight_field_serial", "r2_field")
        self._same(fields, "inflight_radiance_revision", "r2_revision")
        _literal(fields, "field_serial_allocated", "false")

    def mutation_r4(self, fields: dict[str, str]) -> None:
        self.values["r4_revision"] = self.values["r3_revision"] + 1
        self._mutation(fields, self.values["r4_revision"])
        self._same(fields, "inflight_field_serial", "r2_field")
        self._same(fields, "immutable_inflight_radiance_revision", "r2_revision")
        _literal(fields, "latest_coalescing_pending", "true")

    def r4_armed(self, fields: dict[str, str]) -> None:
        self._capture_request(
            fields,
            revision=self.values["r4_revision"],
            building=self.values["r2_field"],
            active_revision=self.values["r1_revision"],
        )
        self._next_frame(fields)

    def r4_capture(self, fields: dict[str, str]) -> None:
        self._resident_capture(fields, self.values["r1_field"], self.values["r1_revision"])

    def r2_publication(self, fields: dict[str, str]) -> None:
        self._publication(fields, "r2_field", "r2_revision")

    def r4_midflight(self, fields: dict[str, str]) -> None:
        self._same(fields, "active_field_serial", "r2_field")
        self._same(fields, "active_radiance_revision", "r2_revision")
        self.values["r4_field"] = _integer(fields, "building_field_serial")
        if self.values["r4_field"] != self.values["r2_field"] + 1:
            raise RadianceLifecycleError("r4 field serial drift")
        self._same(fields, "building_radiance_revision", "r4_revision")
        self._same(fields, "source_field_serial", "r2_field")
        _literal(fields, "building_update_epoch", "0")
        _literal(fields, "r3_coalesced", "true")
        _literal(fields, "old_field_visible", "true")

    def r4_publication(self, fields: dict[str, str]) -> None:
        revision = _integer(fields, "radiance_revision")
        if revision == self.values["r3_revision"]:
            raise RadianceLifecycleError("obsolete r3 publication became visible")
        self._publication(fields, "r4_field", "r4_revision")

    def complete_field(self, fields: dict[str, str]) -> None:
        self._same(fields, "field_serial", "r4_field")
        self._same(fields, "geometry_revision", "geometry_revision")
        self._same(fields, "radiance_revision", "r4_revision")
        self._same(fields, "source_field_serial", "r2_field")
        if _integer(fields, "spacing_voxels") != self.spacing_voxels:
            raise RadianceLifecycleError("complete spacing drift")

    def complete_summary(self, fields: dict[str, str]) -> None:
        _literal(fields, "r3_coalesced", "true")
        _literal(fields, "field_serial_gap_r2_to_r4", "1")
        _literal(fields, "geometry_unchanged", "true")
        _literal(fields, "spacing_unchanged", "true")

    def final_armed(self, fields: dict[str, str]) -> None:
        self._capture_request(
            fields,
            revision=self.values["r4_revision"],
            building=0,
            active_field=self.values["r4_field"],
            active_revision=self.values["r4_revision"],
        )

    def final_capture(self, fields: dict[str, str]) -> None:
        self._resident_capture(fields, self.values["r4_field"], self.values["r4_revision"])
        self._same(fields, "source_field_serial", "r2_field")
        self.values["field_serial"] = self.values["r4_field"]
        self.values["source_field_serial"] = self.values["r2_field"]
        self.values["build_token_serial"] = _integer(fields, "build_token_serial")
        if _integer(fields, "generation_token_serial") != self.values["build_token_serial"]:
            raise RadianceLifecycleError("final generation token drift")

    def finish(self) -> dict[str, int]:
        if self.index != len(RADIANCE_ORDER):
            raise RadianceLifecycleError(
                f"incomplete radiance lifecycle; expected {RADIANCE_ORDER[self.index].value}"
            )
        return {
            name: self.values[name]
            for name in (
                "field_serial",
                "source_field_serial",
                "geometry_revision",
                "build_token_serial",
            )
        }

    def _capture_request(
        self,
        fields: dict[str, str],
        *,
        revision: int,
        building: int,
        active_field: int | None = None,
        active_revision: int | None = None,
    ) -> None:
        expected_field = self.values["r1_field"] if active_field is None else active_field
        expected_active_revision = (
            self.values["r1_revision"] if active_revision is None else active_revision
        )
        if _integer(fields, "active_field_serial") != expected_field:
            raise RadianceLifecycleError("capture active field drift")
        if _integer(fields, "active_radiance_revision") != expected_active_revision:
            raise RadianceLifecycleError("capture active radiance drift")
        if _integer(fields, "building_field_serial") != building:
            raise RadianceLifecycleError("capture building field drift")
        if _integer(fields, "live_radiance_revision") != revision:
            raise RadianceLifecycleError("capture live radiance drift")
        if _integer(fields, "latest_radiance_revision") != revision:
            raise RadianceLifecycleError("capture latest radiance drift")

    @staticmethod
    def _next_frame(fields: dict[str, str]) -> None:
        mutation = _integer(fields, "mutation_frame")
        capture = _integer(fields, "capture_frame")
        if capture != mutation + 1:
            raise RadianceLifecycleError("capture is not the first rendered frame after mutation")

    @staticmethod
    def _mutation(fields: dict[str, str], revision: int) -> None:
        if _integer(fields, "expected_radiance_revision") != revision:
            raise RadianceLifecycleError("mutation revision drift")
        if "after_render_frame" in fields and "first_affected_render_frame" in fields:
            if _integer(fields, "first_affected_render_frame") != _integer(fields, "after_render_frame") + 1:
                raise RadianceLifecycleError("direct sun did not change on the first rendered frame")

    def _resident_capture(self, fields: dict[str, str], field: int, revision: int) -> None:
        if _integer(fields, "field_serial") != field:
            raise RadianceLifecycleError("resident capture field drift")
        if _integer(fields, "geometry_revision") != self.values["geometry_revision"]:
            raise RadianceLifecycleError("resident capture geometry drift")
        if _integer(fields, "radiance_revision") != revision:
            raise RadianceLifecycleError("resident capture radiance drift")
        if _integer(fields, "spacing_voxels") != self.spacing_voxels:
            raise RadianceLifecycleError("resident capture spacing drift")
        _literal(fields, "publication", "Published")

    def _publication(self, fields: dict[str, str], field: str, revision: str) -> None:
        self._same(fields, "serial", field)
        self._same(fields, "geometry_revision", "geometry_revision")
        self._same(fields, "radiance_revision", revision)
        _literal(fields, "kind", "RadianceUpdate")

    def _same(self, fields: dict[str, str], name: str, expected_name: str) -> None:
        actual = _integer(fields, name)
        expected = self.values[expected_name]
        if actual != expected:
            raise RadianceLifecycleError(
                f"radiance field drift for {name}: expected {expected}, got {actual}"
            )


def validate_radiance_event_stream(text: str, spacing_voxels: int) -> dict[str, int]:
    lifecycle = _RadianceStream(spacing_voxels)
    checkpoint_map = {
        "r1-terminal": _RadiancePhase.TERMINAL,
        "baseline": _RadiancePhase.BASELINE_ARMED,
        "r2-next-frame": _RadiancePhase.R2_ARMED,
        "r2-midflight": _RadiancePhase.R2_MIDFLIGHT,
        "r3-observed": _RadiancePhase.R3_OBSERVED,
        "r4-next-frame": _RadiancePhase.R4_ARMED,
        "r4-midflight": _RadiancePhase.R4_MIDFLIGHT,
        "complete": _RadiancePhase.COMPLETE_FIELD,
        "final": _RadiancePhase.FINAL_ARMED,
    }
    mutation_map = {
        "r2": _RadiancePhase.MUTATION_R2,
        "r3": _RadiancePhase.MUTATION_R3,
        "r4": _RadiancePhase.MUTATION_R4,
    }
    for line in text.splitlines():
        fields = _fields(line)
        if "[DDGI_ACCEPT][RADIANCE] checkpoint=" in line:
            checkpoint = fields.get("checkpoint")
            if checkpoint in checkpoint_map:
                lifecycle.event(checkpoint_map[checkpoint], fields)
            continue
        if "[DDGI_ACCEPT][RADIANCE] mutation=" in line:
            mutation = fields.get("mutation")
            if mutation in mutation_map:
                lifecycle.event(mutation_map[mutation], fields)
            continue
        if "[DDGI_ACCEPT][RADIANCE] complete " in line:
            lifecycle.event(_RadiancePhase.COMPLETE_SUMMARY, fields)
            continue
        if "[DDGI][PUBLICATION]" in line and fields.get("kind") == "RadianceUpdate":
            revision = _integer(fields, "radiance_revision")
            if revision == lifecycle.values.get("r2_revision"):
                lifecycle.event(_RadiancePhase.R2_PUBLICATION, fields)
            elif revision == lifecycle.values.get("r4_revision"):
                lifecycle.event(_RadiancePhase.R4_PUBLICATION, fields)
            else:
                raise RadianceLifecycleError(
                    f"obsolete or unknown radiance publication revision={revision}"
                )
            continue
        if "[ENV_IRRADIANCE_CAPTURE] checkpoint target=published" in line:
            capture_phases = (
                _RadiancePhase.BASELINE_CAPTURE,
                _RadiancePhase.R2_CAPTURE,
                _RadiancePhase.R4_CAPTURE,
                _RadiancePhase.FINAL_CAPTURE,
            )
            if lifecycle.captures >= len(capture_phases):
                raise RadianceLifecycleError("duplicate radiance capture publication")
            phase = capture_phases[lifecycle.captures]
            lifecycle.captures += 1
            lifecycle.event(phase, fields)
    return lifecycle.finish()


class DensityLifecycleError(ValueError):
    """The log is not one ordered density replacement lifecycle."""


class _DensityPhase(Enum):
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


DENSITY_ORDER = tuple(_DensityPhase)


@dataclass(frozen=True)
class _DensityCapture:
    fields: dict[str, str]
    arrival: _DensityPhase | None


class _DensityStream:
    def __init__(self) -> None:
        self.index = 0
        self.seen: set[_DensityPhase] = set()
        self.values: dict[str, int] = {}
        self.captures: list[_DensityCapture] = []

    def event(self, phase: _DensityPhase, fields: dict[str, str]) -> None:
        if phase in self.seen:
            raise DensityLifecycleError(f"duplicate lifecycle event {phase.value}")
        expected = DENSITY_ORDER[self.index] if self.index < len(DENSITY_ORDER) else None
        if expected is not phase:
            label = expected.value if expected else "end-of-lifecycle"
            raise DensityLifecycleError(
                f"lifecycle event {phase.value} is out of order; expected {label}"
            )
        self.seen.add(phase)
        self.index += 1
        getattr(self, phase.name.lower())(fields)

    def baseline(self, fields: dict[str, str]) -> None:
        self.values["baseline_field_serial"] = _integer(fields, "field_serial")
        self.values["baseline_geometry_revision"] = _integer(
            fields, "geometry_revision"
        )
        self.values["radiance_revision"] = _integer(fields, "radiance_revision")
        self._literals(
            fields,
            spacing_voxels="32",
            state="Converging",
            update_epoch="0",
            source_field_serial="0",
        )

    def density_midflight(self, fields: dict[str, str]) -> None:
        self._same(fields, "active_field_serial", "baseline_field_serial")
        self._same(
            fields, "active_geometry_revision", "baseline_geometry_revision"
        )
        self._literals(
            fields,
            active_spacing_voxels="32",
            obsolete_density_spacing_voxels="16",
            old_field_visible="true",
            active_available="true",
        )
        for name in (
            "active_token_serial",
            "obsolete_density_token_serial",
            "obsolete_density_field_serial",
        ):
            self.values[name] = _integer(fields, name)

    def geometry_preempted(self, fields: dict[str, str]) -> None:
        self._same(fields, "obsolete_density_token_serial")
        self._same(fields, "obsolete_density_field_serial")
        terrain = _integer(fields, "terrain_token_serial")
        if terrain <= self.values["obsolete_density_token_serial"]:
            raise DensityLifecycleError(
                "terrain token did not supersede the obsolete density token"
            )
        self.values["terrain_token_serial"] = terrain
        self.values["geometry_revision"] = _integer(
            fields, "target_geometry_revision"
        )
        self._literals(
            fields,
            terrain_spacing_voxels="32",
            queued_density_spacing_voxels="16",
            obsolete_density_consumer_visible="false",
            active_available="true",
        )

    def geometry_private(self, fields: dict[str, str]) -> None:
        self._same(fields, "terrain_token_serial")
        self._same(fields, "generation_token_serial", "terrain_token_serial")
        self._same(fields, "obsolete_density_token_serial")
        self._same(fields, "geometry_revision")
        self._same(fields, "radiance_revision")
        source = _integer(fields, "source_field_serial")
        if source != self.values["baseline_field_serial"]:
            raise DensityLifecycleError("geometry epoch-zero source field changed")
        root = _integer(fields, "epoch_zero_field_serial")
        current = _integer(fields, "private_current_field_serial")
        epoch = _integer(fields, "private_current_update_epoch")
        if epoch < 0:
            raise DensityLifecycleError("private current epoch is negative")
        if epoch == 0 and current != root:
            raise DensityLifecycleError(
                "private current field differs from its epoch-zero root"
            )
        self.values.update(
            geometry_epoch_zero_field_serial=root,
            geometry_private_current_field_serial=current,
            geometry_private_current_update_epoch=epoch,
        )
        self._literals(
            fields,
            active_spacing_voxels="32",
            queued_density_spacing_voxels="16",
            obsolete_density_consumer_visible="false",
            active_available="true",
        )

    def terrain_promotion(self, fields: dict[str, str]) -> None:
        self._same(fields, "token_serial", "terrain_token_serial")
        self._same(fields, "generation_token_serial", "terrain_token_serial")
        self._same(fields, "geometry_revision")
        self._same(fields, "radiance_revision")
        self._same(
            fields,
            "epoch_zero_field_serial",
            "geometry_epoch_zero_field_serial",
        )
        self._literals(fields, kind="Terrain", spacing_voxels="32")
        epoch = _integer(fields, "published_update_epoch")
        if epoch == 0:
            raise DensityLifecycleError("raw geometry epoch zero became visible")
        private_epoch = self.values["geometry_private_current_update_epoch"]
        if epoch < private_epoch:
            raise DensityLifecycleError("promotion regressed behind private current epoch")
        published = _integer(fields, "published_field_serial")
        if (
            epoch == private_epoch
            and published != self.values["geometry_private_current_field_serial"]
        ):
            raise DensityLifecycleError(
                "promotion changed the private current field at the same epoch"
            )
        self.values["geometry_published_update_epoch"] = epoch
        self.values["geometry_published_field_serial"] = published

    def terrain_consumers(self, fields: dict[str, str]) -> None:
        for actual, expected in (
            ("active_token_serial", "terrain_token_serial"),
            ("generation_token_serial", "terrain_token_serial"),
            ("geometry_revision", "geometry_revision"),
            ("radiance_revision", "radiance_revision"),
            ("epoch_zero_field_serial", "geometry_epoch_zero_field_serial"),
            ("published_field_serial", "geometry_published_field_serial"),
            ("update_epoch", "geometry_published_update_epoch"),
        ):
            self._same(fields, actual, expected)
        _literal(fields, "spacing_voxels", "32")

    def geometry_published(self, fields: dict[str, str]) -> None:
        for actual, expected in (
            ("terrain_token_serial", "terrain_token_serial"),
            ("generation_token_serial", "terrain_token_serial"),
            ("obsolete_density_token_serial", "obsolete_density_token_serial"),
            ("geometry_revision", "geometry_revision"),
            ("radiance_revision", "radiance_revision"),
            ("epoch_zero_field_serial", "geometry_epoch_zero_field_serial"),
            ("published_field_serial", "geometry_published_field_serial"),
            ("published_update_epoch", "geometry_published_update_epoch"),
        ):
            self._same(fields, actual, expected)
        self._literals(
            fields,
            same_generation="true",
            active_spacing_voxels="32",
            queued_density_spacing_voxels="16",
            obsolete_density_consumer_visible="false",
            active_available="true",
        )

    def density_retry(self, fields: dict[str, str]) -> None:
        for actual, expected in (
            ("active_token_serial", "terrain_token_serial"),
            ("active_field_serial", "geometry_published_field_serial"),
            ("active_geometry_revision", "geometry_revision"),
            ("active_radiance_revision", "radiance_revision"),
            ("density_radiance_revision", "radiance_revision"),
        ):
            self._same(fields, actual, expected)
        self._literals(
            fields,
            active_spacing_voxels="32",
            density_spacing_voxels="16",
            old_field_visible="true",
            active_available="true",
        )
        token = _integer(fields, "density_token_serial")
        if token <= self.values["terrain_token_serial"]:
            raise DensityLifecycleError(
                "retried density token did not supersede terrain token"
            )
        self.values["density_token_serial"] = token
        self.values["density_field_serial"] = _integer(
            fields, "density_field_serial"
        )

    def density_promotion(self, fields: dict[str, str]) -> None:
        for actual, expected in (
            ("token_serial", "density_token_serial"),
            ("generation_token_serial", "density_token_serial"),
            ("geometry_revision", "geometry_revision"),
            ("radiance_revision", "radiance_revision"),
            ("epoch_zero_field_serial", "density_field_serial"),
            ("published_field_serial", "density_field_serial"),
        ):
            self._same(fields, actual, expected)
        self._literals(
            fields,
            kind="Density",
            spacing_voxels="16",
            published_update_epoch="0",
        )

    def density_consumers(self, fields: dict[str, str]) -> None:
        for actual, expected in (
            ("active_token_serial", "density_token_serial"),
            ("generation_token_serial", "density_token_serial"),
            ("geometry_revision", "geometry_revision"),
            ("radiance_revision", "radiance_revision"),
            ("epoch_zero_field_serial", "density_field_serial"),
            ("published_field_serial", "density_field_serial"),
        ):
            self._same(fields, actual, expected)
        self._literals(fields, spacing_voxels="16", update_epoch="0")

    def complete(self, fields: dict[str, str]) -> None:
        self._same(fields, "field_serial", "density_field_serial")
        self._same(fields, "geometry_revision")
        self._same(fields, "radiance_revision")
        self._literals(
            fields,
            spacing_voxels="16",
            state="Converging",
            update_epoch="0",
            source_field_serial="0",
            source_state="none",
        )
        self.values["field_serial"] = _integer(fields, "field_serial")
        self.values["source_field_serial"] = 0

    def summary(self, fields: dict[str, str]) -> None:
        for name in (
            "obsolete_density_token_serial",
            "terrain_token_serial",
            "density_token_serial",
            "geometry_revision",
            "radiance_revision",
        ):
            self._same(fields, name)
        self._literals(
            fields,
            obsolete_density_consumer_visible="false",
            first_consumer_visible_16_epoch="0",
            spacing_voxels="16",
        )

    def promotion(self, fields: dict[str, str]) -> None:
        token = _integer(fields, "token_serial")
        if token == self.values.get("obsolete_density_token_serial"):
            raise DensityLifecycleError("obsolete density token was promoted")
        if token == self.values.get("terrain_token_serial"):
            self.event(_DensityPhase.TERRAIN_PROMOTION, fields)
        elif token == self.values.get("density_token_serial"):
            self.event(_DensityPhase.DENSITY_PROMOTION, fields)
        elif _DensityPhase.BASELINE in self.seen:
            raise DensityLifecycleError(f"mixed-log promotion token {token}")
        else:
            raise DensityLifecycleError(f"unknown promotion token {token}")

    def consumers(self, fields: dict[str, str]) -> None:
        token = _integer(fields, "active_token_serial")
        if token == self.values.get("obsolete_density_token_serial"):
            raise DensityLifecycleError("obsolete density token became active")
        if token == self.values.get("terrain_token_serial"):
            self.event(_DensityPhase.TERRAIN_CONSUMERS, fields)
        elif token == self.values.get("density_token_serial"):
            self.event(_DensityPhase.DENSITY_CONSUMERS, fields)
        elif _DensityPhase.BASELINE in self.seen:
            raise DensityLifecycleError(f"mixed-log consumer token {token}")
        else:
            raise DensityLifecycleError(f"unknown consumer token {token}")

    def capture(self, fields: dict[str, str]) -> None:
        _literal(fields, "target", "e0")
        _required(
            fields,
            "build_token_serial",
            "generation_token_serial",
            "epoch_zero_field_serial",
            "field_serial",
            "source_field_serial",
            "geometry_revision",
            "radiance_revision",
            "spacing_voxels",
            "state",
            "update_epoch",
            "publication",
        )
        arrival = DENSITY_ORDER[self.index] if self.index < len(DENSITY_ORDER) else None
        self.captures.append(_DensityCapture(fields, arrival))

    def finish(self) -> dict[str, int]:
        if self.index != len(DENSITY_ORDER):
            raise DensityLifecycleError(
                f"incomplete density lifecycle; expected {DENSITY_ORDER[self.index].value}"
            )
        tokens = tuple(
            self.values[name]
            for name in (
                "active_token_serial",
                "obsolete_density_token_serial",
                "terrain_token_serial",
                "density_token_serial",
            )
        )
        if any(left >= right for left, right in zip(tokens, tokens[1:])):
            raise DensityLifecycleError(
                "generation tokens must be strictly increasing: active < obsolete "
                "density < terrain < retried density"
            )
        expected = {
            self.values["active_token_serial"]: (
                "baseline",
                self.values["baseline_field_serial"],
                0,
                self.values["baseline_geometry_revision"],
                32,
                _DensityPhase.BASELINE,
            ),
            self.values["terrain_token_serial"]: (
                "terrain",
                self.values["geometry_epoch_zero_field_serial"],
                self.values["baseline_field_serial"],
                self.values["geometry_revision"],
                32,
                _DensityPhase.GEOMETRY_PRIVATE,
            ),
            self.values["density_token_serial"]: (
                "density",
                self.values["density_field_serial"],
                0,
                self.values["geometry_revision"],
                16,
                _DensityPhase.DENSITY_PROMOTION,
            ),
        }
        seen: set[int] = set()
        for capture in self.captures:
            fields = capture.fields
            token = _integer(fields, "build_token_serial")
            if token != _integer(fields, "generation_token_serial"):
                raise DensityLifecycleError("capture generation token drift")
            if token == self.values["obsolete_density_token_serial"]:
                raise DensityLifecycleError("obsolete density capture was published")
            if token not in expected:
                raise DensityLifecycleError(f"unknown capture generation token {token}")
            if token in seen:
                raise DensityLifecycleError(f"duplicate capture for generation token {token}")
            seen.add(token)
            label, root, source, geometry, spacing, arrival = expected[token]
            if capture.arrival is not arrival:
                actual = capture.arrival.value if capture.arrival else "end-of-lifecycle"
                raise DensityLifecycleError(
                    f"{label} capture window expected before {arrival.value}, got {actual}"
                )
            for name in ("epoch_zero_field_serial", "field_serial"):
                if _integer(fields, name) != root:
                    raise DensityLifecycleError(f"{label} capture field drift")
            for name, value in (
                ("source_field_serial", source),
                ("geometry_revision", geometry),
                ("radiance_revision", self.values["radiance_revision"]),
                ("spacing_voxels", spacing),
            ):
                if _integer(fields, name) != value:
                    raise DensityLifecycleError(f"{label} capture {name} drift")
            self._literals(
                fields,
                state="Converging",
                update_epoch="0",
                publication="Published",
            )
        missing = set(expected) - seen
        if missing:
            raise DensityLifecycleError(f"missing capture generations: {sorted(missing)}")
        self.values["build_token_serial"] = self.values["density_token_serial"]
        return dict(self.values)

    def _same(
        self, fields: dict[str, str], actual: str, expected: str | None = None
    ) -> None:
        expected = expected or actual
        value = _integer(fields, actual)
        if value != self.values[expected]:
            label = expected.replace("_serial", "").replace("_", " ")
            raise DensityLifecycleError(
                f"{label} changed: expected {self.values[expected]}, got {value}"
            )

    @staticmethod
    def _literals(fields: dict[str, str], **expected: str) -> None:
        for name, value in expected.items():
            _literal(fields, name, value)


def validate_density_lifecycle(console: Path) -> dict[str, int]:
    lifecycle = _DensityStream()
    checkpoint_map = {
        "baseline": _DensityPhase.BASELINE,
        "density-midflight": _DensityPhase.DENSITY_MIDFLIGHT,
        "geometry-preempted-density": _DensityPhase.GEOMETRY_PREEMPTED,
        "geometry-e0-private": _DensityPhase.GEOMETRY_PRIVATE,
        "geometry-recovery-published": _DensityPhase.GEOMETRY_PUBLISHED,
        "density-retry-midflight": _DensityPhase.DENSITY_RETRY,
        "complete": _DensityPhase.COMPLETE,
    }
    try:
        for line in console.read_text(errors="replace").splitlines():
            fields = _fields(line)
            if "[ENV_IRRADIANCE_CAPTURE] checkpoint " in line:
                lifecycle.capture(fields)
            elif "[DDGI] staging promoted " in line:
                lifecycle.promotion(_fields(line.split(" published_source=", 1)[0]))
            elif "[DDGI][CONSUMERS] consumer_set=" in line:
                lifecycle.consumers(_fields(line.split(" source=", 1)[0]))
            elif "[DDGI_ACCEPT][DENSITY]" in line:
                checkpoint = fields.get("checkpoint")
                if checkpoint in checkpoint_map:
                    lifecycle.event(checkpoint_map[checkpoint], fields)
                elif "[DDGI_ACCEPT][DENSITY] complete " in line:
                    lifecycle.event(_DensityPhase.SUMMARY, fields)
        return lifecycle.finish()
    except DensityLifecycleError:
        raise
    except ValueError as error:
        raise DensityLifecycleError(str(error)) from error


class ScenarioLifecycleError(ValueError):
    """The log is not one ordered terrain-edit evidence lifecycle."""


class _ScenarioKind(Enum):
    PORTAL_READY = "portal-ready"
    CAPTURE_CHECKPOINT = "capture-checkpoint"
    INITIAL = "initial"
    OBSERVED = "observed"
    REQUEST_EDIT = "request-edit"
    PREPARED = "prepared"
    LOCAL_RECOVERY = "local-recovery"
    VISIBLE = "visible"
    OBSOLETE_CANDIDATE = "obsolete-candidate"
    OBSOLETE_SKIPPED = "obsolete-skipped"
    PROMOTION = "promotion"
    CONSUMER = "consumer"
    FLORA = "flora"
    EDIT_READY = "edit-ready"
    DENSITY_REQUEST = "density-request"
    DENSITY_READY = "density-ready"
    COMPLETE = "complete"
    TRANSIENT_ARMED = "transient-armed"
    TRANSIENT_RECORDING = "transient-recording"
    CAPTURE_SAVED = "capture-saved"
    CAPTURE_COMPLETE = "capture-complete"
    CONVERGENCE = "convergence"


@dataclass(frozen=True)
class _ScenarioEvent:
    kind: _ScenarioKind
    fields: dict[str, str]
    line: str
    position: int


class _CyclePhase(Enum):
    PORTAL_READY = "portal-ready"
    ACTIVE_CHECKPOINT = "active-checkpoint"
    INITIAL = "initial"
    OBSERVED_CLOSE = "observed-close"
    REQUEST_CLOSE = "request-close"
    PREPARED_CLOSE = "prepared-close"
    RECOVERY_CLOSE = "recovery-close"
    VISIBLE_CLOSE = "visible-close"
    OBSOLETE_CANDIDATE = "obsolete-candidate"
    OBSERVED_REOPEN = "observed-reopen"
    REQUEST_REOPEN = "request-reopen"
    VISIBLE_REOPEN = "visible-reopen"
    OBSOLETE_SKIPPED = "obsolete-skipped"
    PREPARED_REOPEN = "prepared-reopen"
    RECOVERY_REOPEN = "recovery-reopen"
    PROMOTION_CLOSE = "promotion-close"
    CONSUMER_CLOSE = "consumer-close"
    READY_CLOSE = "ready-close"
    PROMOTION_REOPEN = "promotion-reopen"
    CONSUMER_REOPEN = "consumer-reopen"
    FLORA_REOPEN = "flora-reopen"
    READY_REOPEN = "ready-reopen"
    PREPARED_DENSITY = "prepared-density"
    REQUEST_DENSITY = "request-density"
    PROMOTION_DENSITY = "promotion-density"
    CONSUMER_DENSITY = "consumer-density"
    FLORA_DENSITY = "flora-density"
    READY_DENSITY = "ready-density"
    TRANSIENT_ARMED = "transient-armed"
    TRANSIENT_RECORDING = "transient-recording"
    COMPLETE = "complete"
    FINAL_CHECKPOINT = "final-checkpoint"
    CAPTURE_SAVED = "capture-saved"
    CAPTURE_COMPLETE = "capture-complete"


@dataclass
class _OrderedScenarioStream:
    name: str
    order: tuple[_CyclePhase, ...]
    index: int = 0

    def __post_init__(self) -> None:
        self.events: dict[_CyclePhase, _ScenarioEvent] = {}

    def event(self, phase: _CyclePhase, event: _ScenarioEvent) -> None:
        expected = self.order[self.index] if self.index < len(self.order) else None
        if phase in self.events or expected is not phase:
            label = expected.value if expected is not None else "end-of-lifecycle"
            raise ScenarioLifecycleError(
                f"{self.name} event {phase.value} is duplicate or out of order; "
                f"expected {label}"
            )
        self.events[phase] = event
        self.index += 1

    def finish(self) -> dict[_CyclePhase, _ScenarioEvent]:
        if self.index != len(self.order):
            raise ScenarioLifecycleError(
                f"incomplete {self.name} lifecycle; expected {self.order[self.index].value}"
            )
        return self.events

    def reject(self, event: _ScenarioEvent) -> None:
        expected = self.order[self.index] if self.index < len(self.order) else None
        label = expected.value if expected is not None else "end-of-lifecycle"
        raise ScenarioLifecycleError(
            f"{self.name} has unexpected {event.kind.value} event while expecting {label}: "
            f"{event.line}"
        )


def _scenario_events(text: str) -> tuple[_ScenarioEvent, ...]:
    events: list[_ScenarioEvent] = []
    for position, line in enumerate(text.splitlines()):
        kind: _ScenarioKind | None = None
        if "[ENV_LIGHT_TEST] ready case=portal backend=ddgi" in line:
            kind = _ScenarioKind.PORTAL_READY
        elif "[ENV_IRRADIANCE_CAPTURE] checkpoint " in line:
            kind = _ScenarioKind.CAPTURE_CHECKPOINT
        elif "[ENV_LIGHT_EDIT_CYCLE] initial probe field ready " in line:
            kind = _ScenarioKind.INITIAL
        elif "[DDGI] runtime observed visible terrain revision=" in line:
            kind = _ScenarioKind.OBSERVED
        elif "[ENV_LIGHT_EDIT_CYCLE] requested edit=" in line:
            kind = _ScenarioKind.REQUEST_EDIT
        elif "[DDGI] staging prepared " in line:
            kind = _ScenarioKind.PREPARED
        elif "[DDGI][LOCAL_RECOVERY] prepared " in line:
            kind = _ScenarioKind.LOCAL_RECOVERY
        elif "[ENV_LIGHT_EDIT_CYCLE] visible terrain publication complete " in line:
            kind = _ScenarioKind.VISIBLE
        elif "[ENV_LIGHT_EDIT_INFLIGHT] obsolete candidate observed " in line:
            kind = _ScenarioKind.OBSOLETE_CANDIDATE
        elif (
            "[DDGI] obsolete staging promotion skipped " in line
            and "replacement_token_serial=" in line
        ):
            kind = _ScenarioKind.OBSOLETE_SKIPPED
        elif "[DDGI] staging promoted " in line:
            kind = _ScenarioKind.PROMOTION
        elif "[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster " in line:
            kind = _ScenarioKind.CONSUMER
        elif "[DDGI][FLORA_CONSUMER] draw_recorded " in line:
            kind = _ScenarioKind.FLORA
        elif "[ENV_LIGHT_EDIT_CYCLE] edited probe field ready " in line:
            kind = _ScenarioKind.EDIT_READY
        elif "[ENV_LIGHT_EDIT_CYCLE] requested density rebuild " in line:
            kind = _ScenarioKind.DENSITY_REQUEST
        elif "[ENV_LIGHT_EDIT_CYCLE] density rebuild ready " in line:
            kind = _ScenarioKind.DENSITY_READY
        elif "[ENV_LIGHT_EDIT_CYCLE] complete mode=" in line:
            kind = _ScenarioKind.COMPLETE
        elif "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] armed " in line:
            kind = _ScenarioKind.TRANSIENT_ARMED
        elif "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] recording " in line:
            kind = _ScenarioKind.TRANSIENT_RECORDING
        elif "[ENV_IRRADIANCE_CAPTURE] saved " in line:
            kind = _ScenarioKind.CAPTURE_SAVED
        elif "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run" in line:
            kind = _ScenarioKind.CAPTURE_COMPLETE
        elif "[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated" in line:
            kind = _ScenarioKind.CONVERGENCE
        if kind is None:
            continue
        fields = _fields(line)
        if kind is _ScenarioKind.OBSOLETE_CANDIDATE:
            token = re.search(
                r"token=Some\(DdgiBuildToken \{ serial: (\d+), .*?spacing_voxels: (\d+)",
                line,
            )
            if token is not None:
                fields.setdefault("token_serial", token.group(1))
                fields.setdefault("spacing_voxels", token.group(2))
        events.append(_ScenarioEvent(kind, fields, line, position))
    return tuple(events)


def _one_initial(events: tuple[_ScenarioEvent, ...]) -> int:
    initial = next((event for event in events if event.kind is _ScenarioKind.INITIAL), None)
    if initial is None:
        raise ScenarioLifecycleError("incomplete terrain edit lifecycle; expected initial")
    return _integer(initial.fields, "terrain_revision")


def _same_identity(
    event: _ScenarioEvent, name: str, expected: int, label: str
) -> None:
    actual = _integer(event.fields, name)
    if actual != expected:
        raise ScenarioLifecycleError(
            f"{label} identity drift for {name}: expected {expected}, got {actual}"
        )


def _same_literal(
    event: _ScenarioEvent, name: str, expected: str, label: str
) -> None:
    try:
        _literal(event.fields, name, expected)
    except ValueError as error:
        raise ScenarioLifecycleError(f"{label} identity drift: {error}") from error


@dataclass(frozen=True)
class _PublishedFieldIdentity:
    build_token: int
    generation_token: int
    geometry_revision: int
    radiance_revision: int
    spacing_voxels: int
    epoch_zero_field: int
    published_field: int

    @classmethod
    def from_promotion(
        cls,
        event: _ScenarioEvent,
        *,
        build_token: int,
        geometry_revision: int,
        spacing_voxels: int,
        label: str,
    ) -> _PublishedFieldIdentity:
        for name, expected in (
            ("token_serial", build_token),
            ("generation_token_serial", build_token),
            ("geometry_revision", geometry_revision),
            ("spacing_voxels", spacing_voxels),
        ):
            _same_identity(event, name, expected, label)
        epoch_zero = _integer(event.fields, "epoch_zero_field_serial")
        published = _integer(event.fields, "published_field_serial")
        if epoch_zero <= 0 or published < epoch_zero:
            raise ScenarioLifecycleError(
                f"{label} field identity drift: epoch-zero={epoch_zero}, "
                f"published={published}"
            )
        return cls(
            build_token,
            build_token,
            geometry_revision,
            _integer(event.fields, "radiance_revision"),
            spacing_voxels,
            epoch_zero,
            published,
        )

    def require_consumer(self, event: _ScenarioEvent, label: str) -> None:
        for name, expected in (
            ("active_token_serial", self.build_token),
            ("generation_token_serial", self.generation_token),
            ("geometry_revision", self.geometry_revision),
            ("radiance_revision", self.radiance_revision),
            ("spacing_voxels", self.spacing_voxels),
            ("epoch_zero_field_serial", self.epoch_zero_field),
            ("published_field_serial", self.published_field),
        ):
            _same_identity(event, name, expected, label)

    def require_checkpoint(self, event: _ScenarioEvent, label: str) -> None:
        for name, expected in (
            ("build_token_serial", self.build_token),
            ("generation_token_serial", self.generation_token),
            ("geometry_revision", self.geometry_revision),
            ("radiance_revision", self.radiance_revision),
            ("spacing_voxels", self.spacing_voxels),
            ("epoch_zero_field_serial", self.epoch_zero_field),
        ):
            _same_identity(event, name, expected, label)
        field = _integer(event.fields, "field_serial")
        if field < self.published_field:
            raise ScenarioLifecycleError(
                f"{label} field identity drift: checkpoint field {field} precedes "
                f"published field {self.published_field}"
            )


@dataclass(frozen=True)
class _CheckpointIdentity:
    build_token: int
    generation_token: int
    epoch_zero_field: int
    field: int
    source_field: int
    geometry_revision: int
    radiance_revision: int
    spacing_voxels: int
    state: str
    update_epoch: int
    publication: str

    @classmethod
    def from_event(
        cls,
        action: ValidateScenarioLog,
        event: _ScenarioEvent,
        label: str,
    ) -> _CheckpointIdentity:
        fields = event.fields
        identity = cls(
            build_token=_integer(fields, "build_token_serial"),
            generation_token=_integer(fields, "generation_token_serial"),
            epoch_zero_field=_integer(fields, "epoch_zero_field_serial"),
            field=_integer(fields, "field_serial"),
            source_field=_integer(fields, "source_field_serial"),
            geometry_revision=_integer(fields, "geometry_revision"),
            radiance_revision=_integer(fields, "radiance_revision"),
            spacing_voxels=_integer(fields, "spacing_voxels"),
            state=fields.get("state", ""),
            update_epoch=_integer(fields, "update_epoch"),
            publication=fields.get("publication", ""),
        )
        if identity.generation_token != identity.build_token:
            raise ScenarioLifecycleError(
                f"{label} identity drift: build token {identity.build_token} does not "
                f"own generation {identity.generation_token}"
            )
        if identity.spacing_voxels != action.spacing_voxels:
            raise ScenarioLifecycleError(
                f"{label} identity drift for spacing_voxels: expected "
                f"{action.spacing_voxels}, got {identity.spacing_voxels}"
            )
        if identity.epoch_zero_field <= 0 or identity.field < identity.epoch_zero_field:
            raise ScenarioLifecycleError(
                f"{label} field identity drift: epoch-zero={identity.epoch_zero_field}, "
                f"field={identity.field}"
            )
        if identity.state not in {"Converging", "Converged"}:
            raise ScenarioLifecycleError(
                f"{label} identity drift: unsupported state {identity.state!r}"
            )
        if identity.update_epoch < 0:
            raise ScenarioLifecycleError(
                f"{label} identity drift: negative update epoch"
            )
        if identity.publication != "Published":
            raise ScenarioLifecycleError(
                f"{label} identity drift: publication is {identity.publication!r}"
            )
        return identity

    def require_ready(
        self,
        event: _ScenarioEvent,
        revision_field: str,
        label: str,
    ) -> None:
        _same_identity(event, revision_field, self.geometry_revision, label)

    def require_saved(self, event: _ScenarioEvent, label: str) -> None:
        for name, expected in (
            ("build_token_serial", self.build_token),
            ("field_serial", self.field),
            ("geometry_revision", self.geometry_revision),
            ("radiance_revision", self.radiance_revision),
            ("spacing_voxels", self.spacing_voxels),
        ):
            _same_identity(event, name, expected, label)

    def require_initial_open(self, event: _ScenarioEvent) -> None:
        _same_literal(event, "target", "e8", "initial checkpoint")
        if self.update_epoch < 8:
            raise ScenarioLifecycleError(
                "initial checkpoint identity drift: target e8 was not reached"
            )
        if (
            self.field != self.epoch_zero_field + self.update_epoch
            or self.source_field != self.field - 1
        ):
            raise ScenarioLifecycleError(
                "initial checkpoint field identity has an illegal epoch progression"
            )

    def require_transient_active(self, event: _ScenarioEvent) -> None:
        _same_literal(event, "target", "published", "transient checkpoint")
        if (
            self.state != "Converging"
            or self.update_epoch != 0
            or self.source_field != 0
            or self.field != self.epoch_zero_field
        ):
            raise ScenarioLifecycleError(
                "transient checkpoint identity drift: active publication is not epoch-zero"
            )


@dataclass(frozen=True)
class _CompletedCycle:
    initial_revision: int
    final_revision: int
    final_token: int
    final_field: int
    radiance_revision: int
    close_recovery: _ScenarioEvent
    close_promotion: _ScenarioEvent


def _completed_cycle_order(
    mode: str, *, inflight: bool, require_flora: bool
) -> tuple[_CyclePhase, ...]:
    phases = [
        _CyclePhase.INITIAL,
        _CyclePhase.OBSERVED_CLOSE,
        _CyclePhase.REQUEST_CLOSE,
        _CyclePhase.PREPARED_CLOSE,
        _CyclePhase.RECOVERY_CLOSE,
        _CyclePhase.VISIBLE_CLOSE,
    ]
    if mode == "closed":
        phases.extend(
            (
                _CyclePhase.PROMOTION_CLOSE,
                _CyclePhase.CONSUMER_CLOSE,
                _CyclePhase.READY_CLOSE,
            )
        )
    else:
        if inflight:
            phases.append(_CyclePhase.OBSOLETE_CANDIDATE)
        else:
            phases.extend(
                (
                    _CyclePhase.PROMOTION_CLOSE,
                    _CyclePhase.CONSUMER_CLOSE,
                    _CyclePhase.READY_CLOSE,
                )
            )
        phases.extend(
            (
                _CyclePhase.OBSERVED_REOPEN,
                _CyclePhase.REQUEST_REOPEN,
            )
        )
        if inflight:
            phases.extend(
                (
                    _CyclePhase.VISIBLE_REOPEN,
                    _CyclePhase.OBSOLETE_SKIPPED,
                    _CyclePhase.PREPARED_REOPEN,
                    _CyclePhase.RECOVERY_REOPEN,
                )
            )
        else:
            phases.extend(
                (
                    _CyclePhase.PREPARED_REOPEN,
                    _CyclePhase.RECOVERY_REOPEN,
                    _CyclePhase.VISIBLE_REOPEN,
                )
            )
        phases.extend(
            (
                _CyclePhase.PROMOTION_REOPEN,
                _CyclePhase.CONSUMER_REOPEN,
            )
        )
        if require_flora:
            phases.append(_CyclePhase.FLORA_REOPEN)
        phases.extend(
            (
                _CyclePhase.READY_REOPEN,
                _CyclePhase.PREPARED_DENSITY,
                _CyclePhase.REQUEST_DENSITY,
                _CyclePhase.PROMOTION_DENSITY,
                _CyclePhase.CONSUMER_DENSITY,
            )
        )
        if require_flora:
            phases.append(_CyclePhase.FLORA_DENSITY)
        phases.append(_CyclePhase.READY_DENSITY)
    phases.extend(
        (
            _CyclePhase.COMPLETE,
            _CyclePhase.FINAL_CHECKPOINT,
            _CyclePhase.CAPTURE_SAVED,
            _CyclePhase.CAPTURE_COMPLETE,
        )
    )
    return tuple(phases)


def _validate_completed_cycle(
    action: ValidateScenarioLog,
    text: str,
    *,
    mode: str,
    inflight: bool = False,
    require_flora: bool = False,
    allow_convergence: bool = False,
) -> _CompletedCycle:
    events = _scenario_events(text)
    initial_event = next(
        (event for event in events if event.kind is _ScenarioKind.INITIAL), None
    )
    if initial_event is None:
        raise ScenarioLifecycleError(
            "incomplete terrain edit lifecycle; expected initial"
        )
    events = tuple(
        event for event in events if event.position >= initial_event.position
    )
    initial = _integer(initial_event.fields, "terrain_revision")
    close = initial + 1
    final = close if mode == "closed" else close + 1
    stream = _OrderedScenarioStream(
        f"{mode} terrain edit",
        _completed_cycle_order(mode, inflight=inflight, require_flora=require_flora),
    )
    tokens: dict[str, int] = {}
    for event in events:
        fields = event.fields
        phase: _CyclePhase | None = None
        if event.kind is _ScenarioKind.INITIAL:
            phase = _CyclePhase.INITIAL
        elif event.kind is _ScenarioKind.OBSERVED:
            revision = _integer(fields, "revision")
            if revision == close:
                phase = _CyclePhase.OBSERVED_CLOSE
            elif revision == final and mode != "closed":
                phase = _CyclePhase.OBSERVED_REOPEN
        elif event.kind is _ScenarioKind.REQUEST_EDIT:
            phase = {
                "close-skylight": _CyclePhase.REQUEST_CLOSE,
                "reopen-skylight": _CyclePhase.REQUEST_REOPEN,
            }.get(fields.get("edit"))
        elif event.kind is _ScenarioKind.PREPARED:
            kind = fields.get("kind")
            target = _integer(fields, "target_terrain_revision")
            if kind == "Terrain" and target == close:
                phase = _CyclePhase.PREPARED_CLOSE
            elif kind == "Terrain" and target == final and mode != "closed":
                phase = _CyclePhase.PREPARED_REOPEN
            elif kind == "Density" and target == final and mode != "closed":
                phase = _CyclePhase.PREPARED_DENSITY
        elif event.kind is _ScenarioKind.LOCAL_RECOVERY:
            geometry = _integer(fields, "geometry_revision")
            if geometry == close:
                phase = _CyclePhase.RECOVERY_CLOSE
            elif geometry == final and mode != "closed":
                phase = _CyclePhase.RECOVERY_REOPEN
        elif event.kind is _ScenarioKind.VISIBLE:
            phase = {
                "close-skylight": _CyclePhase.VISIBLE_CLOSE,
                "reopen-skylight": _CyclePhase.VISIBLE_REOPEN,
            }.get(fields.get("edit"))
        elif event.kind is _ScenarioKind.OBSOLETE_CANDIDATE:
            phase = _CyclePhase.OBSOLETE_CANDIDATE
        elif event.kind is _ScenarioKind.OBSOLETE_SKIPPED:
            phase = _CyclePhase.OBSOLETE_SKIPPED
        elif event.kind is _ScenarioKind.PROMOTION:
            kind = fields.get("kind")
            geometry = _integer(fields, "geometry_revision")
            if kind == "Terrain" and geometry == close:
                phase = _CyclePhase.PROMOTION_CLOSE
            elif kind == "Terrain" and geometry == final and mode != "closed":
                phase = _CyclePhase.PROMOTION_REOPEN
            elif kind == "Density" and geometry == final and mode != "closed":
                phase = _CyclePhase.PROMOTION_DENSITY
        elif event.kind is _ScenarioKind.CONSUMER:
            token = _integer(fields, "active_token_serial")
            if token == tokens.get("close"):
                phase = _CyclePhase.CONSUMER_CLOSE
            elif token == tokens.get("reopen"):
                phase = _CyclePhase.CONSUMER_REOPEN
            elif token == tokens.get("density"):
                phase = _CyclePhase.CONSUMER_DENSITY
            elif _integer(fields, "geometry_revision") in (close, final):
                raise ScenarioLifecycleError(
                    f"consumer token identity drift: unknown active token {token}"
                )
        elif event.kind is _ScenarioKind.FLORA and require_flora:
            token = _integer(fields, "active_token_serial")
            if token == tokens.get("reopen"):
                phase = _CyclePhase.FLORA_REOPEN
            elif token == tokens.get("density"):
                phase = _CyclePhase.FLORA_DENSITY
            elif _integer(fields, "terrain_revision") == final and "density" in tokens:
                if token != tokens["density"]:
                    raise ScenarioLifecycleError("flora token identity drift")
        elif event.kind is _ScenarioKind.EDIT_READY:
            phase = {
                "close-skylight": _CyclePhase.READY_CLOSE,
                "reopen-skylight": _CyclePhase.READY_REOPEN,
            }.get(fields.get("edit"))
        elif event.kind is _ScenarioKind.DENSITY_REQUEST:
            phase = _CyclePhase.REQUEST_DENSITY
        elif event.kind is _ScenarioKind.DENSITY_READY:
            phase = _CyclePhase.READY_DENSITY
        elif event.kind is _ScenarioKind.COMPLETE:
            phase = _CyclePhase.COMPLETE
        elif event.kind is _ScenarioKind.CAPTURE_CHECKPOINT:
            phase = _CyclePhase.FINAL_CHECKPOINT
        elif event.kind is _ScenarioKind.CAPTURE_SAVED:
            phase = _CyclePhase.CAPTURE_SAVED
        elif event.kind is _ScenarioKind.CAPTURE_COMPLETE:
            phase = _CyclePhase.CAPTURE_COMPLETE
        if phase is None and allow_convergence and event.kind is _ScenarioKind.CONVERGENCE:
            continue
        if phase is None:
            stream.reject(event)
        stream.event(phase, event)
        if phase is _CyclePhase.PROMOTION_CLOSE:
            tokens["close"] = _integer(fields, "token_serial")
        elif phase is _CyclePhase.PROMOTION_REOPEN:
            tokens["reopen"] = _integer(fields, "token_serial")
        elif phase is _CyclePhase.PROMOTION_DENSITY:
            tokens["density"] = _integer(fields, "token_serial")
    evidence = stream.finish()

    _same_identity(evidence[_CyclePhase.INITIAL], "terrain_revision", initial, "initial")
    close_revision_phases = [
        (_CyclePhase.OBSERVED_CLOSE, close),
        (_CyclePhase.VISIBLE_CLOSE, close),
    ]
    if _CyclePhase.READY_CLOSE in evidence:
        close_revision_phases.append((_CyclePhase.READY_CLOSE, close))
    for phase, revision in close_revision_phases:
        name = "revision" if phase is _CyclePhase.OBSERVED_CLOSE else "target_revision"
        if phase is _CyclePhase.READY_CLOSE:
            name = "terrain_revision"
        _same_identity(evidence[phase], name, revision, phase.value)
    request_close = evidence[_CyclePhase.REQUEST_CLOSE]
    _same_identity(request_close, "source_revision", initial, "close request")
    _same_identity(request_close, "target_revision", close, "close request")
    prepared_close = evidence[_CyclePhase.PREPARED_CLOSE]
    _same_identity(prepared_close, "active_terrain_revision", initial, "close preparation")
    _same_identity(prepared_close, "target_terrain_revision", close, "close preparation")
    close_prepared_token = _integer(prepared_close.fields, "token_serial")
    close_recovery = evidence[_CyclePhase.RECOVERY_CLOSE]
    _same_identity(close_recovery, "geometry_revision", close, "close recovery")
    dirty = _integer(close_recovery.fields, "dirty_probes")
    preserved = _integer(close_recovery.fields, "preserved_probes")
    if dirty <= 0 or preserved <= 0:
        raise ScenarioLifecycleError(
            "close recovery identity requires nonempty dirty and preserved partitions"
        )
    close_promotion = evidence.get(_CyclePhase.PROMOTION_CLOSE)
    close_identity: _PublishedFieldIdentity | None = None
    if close_promotion is not None:
        close_identity = _PublishedFieldIdentity.from_promotion(
            close_promotion,
            build_token=close_prepared_token,
            geometry_revision=close,
            spacing_voxels=action.spacing_voxels,
            label="close promotion",
        )
        _same_literal(close_promotion, "published_state", "Converging", "close promotion")
        if "published_source=Some(" not in close_promotion.line:
            raise ScenarioLifecycleError("close promotion identity lacks history source")
        close_epoch = _integer(close_promotion.fields, "published_update_epoch")
        if close_epoch < action.minimum_epoch:
            raise ScenarioLifecycleError("close promotion identity has insufficient recovery")
        close_consumer = evidence[_CyclePhase.CONSUMER_CLOSE]
        close_identity.require_consumer(close_consumer, "close consumer")
        _same_identity(close_consumer, "update_epoch", close_epoch, "close consumer")

    final_token = close_prepared_token
    final_promotion = close_promotion
    final_identity = close_identity
    if mode != "closed":
        for phase, revision in (
            (_CyclePhase.OBSERVED_REOPEN, final),
            (_CyclePhase.VISIBLE_REOPEN, final),
            (_CyclePhase.READY_REOPEN, final),
            (_CyclePhase.READY_DENSITY, final),
        ):
            name = "revision" if phase is _CyclePhase.OBSERVED_REOPEN else "target_revision"
            if phase in (_CyclePhase.READY_REOPEN, _CyclePhase.READY_DENSITY):
                name = "terrain_revision"
            _same_identity(evidence[phase], name, revision, phase.value)
        request_reopen = evidence[_CyclePhase.REQUEST_REOPEN]
        _same_identity(request_reopen, "source_revision", close, "reopen request")
        _same_identity(request_reopen, "target_revision", final, "reopen request")
        prepared_reopen = evidence[_CyclePhase.PREPARED_REOPEN]
        expected_active = initial if inflight else close
        _same_identity(
            prepared_reopen,
            "active_terrain_revision",
            expected_active,
            "reopen preparation",
        )
        _same_identity(prepared_reopen, "target_terrain_revision", final, "reopen preparation")
        reopen_token = _integer(prepared_reopen.fields, "token_serial")
        if reopen_token <= close_prepared_token:
            raise ScenarioLifecycleError("reopen token identity did not supersede close token")
        if inflight:
            candidate = evidence[_CyclePhase.OBSOLETE_CANDIDATE]
            skipped = evidence[_CyclePhase.OBSOLETE_SKIPPED]
            for event, name, expected in (
                (candidate, "terrain_revision", close),
                (candidate, "active_terrain_revision", initial),
                (candidate, "token_serial", close_prepared_token),
                (skipped, "token_serial", close_prepared_token),
                (skipped, "terrain_revision", close),
                (skipped, "replacement_token_serial", reopen_token),
                (skipped, "replacement_terrain_revision", final),
            ):
                _same_identity(event, name, expected, "latest-wins")
        recovery_reopen = evidence[_CyclePhase.RECOVERY_REOPEN]
        _same_identity(recovery_reopen, "geometry_revision", final, "reopen recovery")
        if _integer(recovery_reopen.fields, "dirty_probes") <= 0 or _integer(
            recovery_reopen.fields, "preserved_probes"
        ) <= 0:
            raise ScenarioLifecycleError("reopen recovery identity lacks a partition")
        promotion_reopen = evidence[_CyclePhase.PROMOTION_REOPEN]
        reopen_identity = _PublishedFieldIdentity.from_promotion(
            promotion_reopen,
            build_token=reopen_token,
            geometry_revision=final,
            spacing_voxels=action.spacing_voxels,
            label="reopen promotion",
        )
        reopen_epoch = _integer(promotion_reopen.fields, "published_update_epoch")
        if reopen_epoch < action.minimum_epoch:
            raise ScenarioLifecycleError("reopen promotion identity has insufficient recovery")
        if "published_source=Some(" not in promotion_reopen.line:
            raise ScenarioLifecycleError("reopen promotion identity lacks history source")
        consumer_reopen = evidence[_CyclePhase.CONSUMER_REOPEN]
        reopen_identity.require_consumer(consumer_reopen, "reopen consumer")
        _same_identity(consumer_reopen, "update_epoch", reopen_epoch, "reopen consumer")
        if require_flora:
            flora_reopen = evidence[_CyclePhase.FLORA_REOPEN]
            for name, expected in (
                ("active_token_serial", reopen_token),
                ("terrain_revision", final),
                ("spacing_voxels", action.spacing_voxels),
            ):
                _same_identity(flora_reopen, name, expected, "reopen flora consumer")
        prepared_density = evidence[_CyclePhase.PREPARED_DENSITY]
        for name, expected in (
            ("active_terrain_revision", final),
            ("target_terrain_revision", final),
            ("spacing_voxels", action.spacing_voxels),
        ):
            _same_identity(prepared_density, name, expected, "density preparation")
        density_token = _integer(prepared_density.fields, "token_serial")
        if density_token <= reopen_token:
            raise ScenarioLifecycleError("density token identity did not supersede reopen token")
        density_request = evidence[_CyclePhase.REQUEST_DENSITY]
        _same_identity(density_request, "terrain_revision", final, "density request")
        _same_identity(density_request, "spacing_voxels", action.spacing_voxels, "density request")
        density_promotion = evidence[_CyclePhase.PROMOTION_DENSITY]
        density_identity = _PublishedFieldIdentity.from_promotion(
            density_promotion,
            build_token=density_token,
            geometry_revision=final,
            spacing_voxels=action.spacing_voxels,
            label="density promotion",
        )
        _same_identity(density_promotion, "published_update_epoch", 0, "density promotion")
        _same_literal(density_promotion, "published_source", "None", "density promotion")
        if density_identity.published_field != density_identity.epoch_zero_field:
            raise ScenarioLifecycleError(
                "density promotion field identity drift: e0 publication is not epoch-zero"
            )
        density_consumer = evidence[_CyclePhase.CONSUMER_DENSITY]
        density_identity.require_consumer(density_consumer, "density consumer")
        _same_identity(density_consumer, "update_epoch", 0, "density consumer")
        if require_flora:
            flora = evidence[_CyclePhase.FLORA_DENSITY]
            for name, expected in (
                ("active_token_serial", density_token),
                ("terrain_revision", final),
                ("spacing_voxels", action.spacing_voxels),
            ):
                _same_identity(flora, name, expected, "flora consumer")
            if _integer(flora.fields, "instance_count") <= 0:
                raise ScenarioLifecycleError("flora consumer identity has no instances")
        final_token = density_token
        final_promotion = density_promotion
        final_identity = density_identity

    complete = evidence[_CyclePhase.COMPLETE]
    _same_literal(complete, "mode", mode, "cycle completion")
    _same_identity(complete, "final_terrain_revision", final, "cycle completion")
    checkpoint = evidence[_CyclePhase.FINAL_CHECKPOINT]
    saved = evidence[_CyclePhase.CAPTURE_SAVED]
    assert final_identity is not None
    final_identity.require_checkpoint(checkpoint, "capture checkpoint")
    _same_identity(saved, "build_token_serial", final_token, "capture save")
    final_field = _integer(checkpoint.fields, "field_serial")
    _same_identity(saved, "field_serial", final_field, "capture save")
    _same_identity(saved, "geometry_revision", final, "capture save")
    _same_identity(saved, "spacing_voxels", action.spacing_voxels, "capture save")
    assert final_promotion is not None
    radiance = _integer(final_promotion.fields, "radiance_revision")
    _same_identity(saved, "radiance_revision", radiance, "capture save")
    return _CompletedCycle(
        initial,
        final,
        final_token,
        final_field,
        radiance,
        close_recovery,
        close_promotion or final_promotion,
    )


def _validate_inflight(action: ValidateScenarioLog, text: str) -> dict[str, int]:
    cycle = _validate_completed_cycle(
        action, text, mode="reopened", inflight=True
    )
    return {
        "final_revision": cycle.final_revision,
        "spacing_voxels": action.spacing_voxels,
        "build_token_serial": cycle.final_token,
        "field_serial": cycle.final_field,
    }


def _validate_cycle(action: ValidateScenarioLog, text: str) -> dict[str, int]:
    cycle = _validate_completed_cycle(action, text, mode=action.state)
    return {
        "final_revision": cycle.final_revision,
        "build_token_serial": cycle.final_token,
        "field_serial": cycle.final_field,
    }


def _validate_local(action: ValidateScenarioLog, text: str) -> dict[str, int]:
    cycle = _validate_completed_cycle(
        action,
        text,
        mode="closed",
        allow_convergence=True,
    )
    if "invalidation_voxel_bound=Some((UVec3(0, 0, 0), UVec3(512, 512, 512)))" in text:
        raise ScenarioLifecycleError("local recovery identity invalidated the full DDGI domain")
    recovery = cycle.close_recovery
    promotion = cycle.close_promotion
    high_delta = sum(
        event.position > promotion.position
        and _integer(event.fields, "geometry_revision") == cycle.final_revision
        and float(event.fields.get("max_abs_rgb_delta", "0")) > 0.1
        for event in _scenario_events(text)
        if event.kind is _ScenarioKind.CONVERGENCE
    )
    if high_delta > action.maximum_high_delta_epochs:
        raise ScenarioLifecycleError(
            f"local recovery identity has {high_delta} high-delta epochs after promotion"
        )
    return {
        "final_revision": cycle.final_revision,
        "dirty_probes": _integer(recovery.fields, "dirty_probes"),
        "preserved_probes": _integer(recovery.fields, "preserved_probes"),
        "promoted_epoch": _integer(promotion.fields, "published_update_epoch"),
        "post_promotion_high_delta_epochs": high_delta,
    }


def _validate_initial_open(action: ValidateScenarioLog, text: str) -> dict[str, int]:
    stream = _OrderedScenarioStream(
        "initial open",
        (
            _CyclePhase.PORTAL_READY,
            _CyclePhase.FINAL_CHECKPOINT,
            _CyclePhase.CAPTURE_SAVED,
            _CyclePhase.CAPTURE_COMPLETE,
        ),
    )
    events = _scenario_events(text)
    ready_event = next(
        (event for event in events if event.kind is _ScenarioKind.PORTAL_READY),
        None,
    )
    if ready_event is None:
        raise ScenarioLifecycleError("incomplete initial open lifecycle; expected portal-ready")
    for event in (
        event for event in events if event.position >= ready_event.position
    ):
        phase = {
            _ScenarioKind.PORTAL_READY: _CyclePhase.PORTAL_READY,
            _ScenarioKind.CAPTURE_CHECKPOINT: _CyclePhase.FINAL_CHECKPOINT,
            _ScenarioKind.CAPTURE_SAVED: _CyclePhase.CAPTURE_SAVED,
            _ScenarioKind.CAPTURE_COMPLETE: _CyclePhase.CAPTURE_COMPLETE,
        }.get(event.kind)
        if phase is None:
            stream.reject(event)
        stream.event(phase, event)
    evidence = stream.finish()
    ready = evidence[_CyclePhase.PORTAL_READY]
    checkpoint_event = evidence[_CyclePhase.FINAL_CHECKPOINT]
    saved = evidence[_CyclePhase.CAPTURE_SAVED]
    _same_literal(ready, "geometry", "static", "initial open")
    checkpoint = _CheckpointIdentity.from_event(
        action,
        checkpoint_event,
        "initial checkpoint",
    )
    checkpoint.require_initial_open(checkpoint_event)
    checkpoint.require_ready(ready, "terrain_revision", "initial ready")
    checkpoint.require_saved(saved, "initial capture")
    return {"final_revision": checkpoint.geometry_revision}


def _runtime_final(action: ValidateScenarioLog, text: str) -> dict[str, int]:
    if action.state == "initial-open":
        return _validate_initial_open(action, text)
    mode = "closed" if action.state == "closed" else "reopened"
    cycle = _validate_completed_cycle(
        action,
        text,
        mode=mode,
        inflight=action.state == "inflight-latest-wins",
    )
    return {"final_revision": cycle.final_revision}


def _runtime_transient(action: ValidateScenarioLog, text: str) -> dict[str, int]:
    events = _scenario_events(text)
    checkpoint_event = next(
        (
            event
            for event in events
            if event.kind is _ScenarioKind.CAPTURE_CHECKPOINT
        ),
        None,
    )
    if checkpoint_event is None:
        raise ScenarioLifecycleError(
            "incomplete transient terrain edit lifecycle; expected active-checkpoint"
        )
    events = tuple(
        event for event in events if event.position >= checkpoint_event.position
    )
    initial = _one_initial(events)
    close, target = initial + 1, initial + 2
    order = (
        _CyclePhase.ACTIVE_CHECKPOINT,
        _CyclePhase.INITIAL,
        _CyclePhase.OBSERVED_CLOSE,
        _CyclePhase.REQUEST_CLOSE,
        _CyclePhase.PREPARED_CLOSE,
        _CyclePhase.RECOVERY_CLOSE,
        _CyclePhase.VISIBLE_CLOSE,
        _CyclePhase.OBSOLETE_CANDIDATE,
        _CyclePhase.OBSERVED_REOPEN,
        _CyclePhase.REQUEST_REOPEN,
        _CyclePhase.VISIBLE_REOPEN,
        _CyclePhase.OBSOLETE_SKIPPED,
        _CyclePhase.PREPARED_REOPEN,
        _CyclePhase.RECOVERY_REOPEN,
        _CyclePhase.TRANSIENT_ARMED,
        _CyclePhase.TRANSIENT_RECORDING,
        _CyclePhase.CAPTURE_SAVED,
        _CyclePhase.CAPTURE_COMPLETE,
    )
    stream = _OrderedScenarioStream("transient terrain edit", order)
    for event in events:
        fields = event.fields
        phase: _CyclePhase | None = None
        if event.kind is _ScenarioKind.CAPTURE_CHECKPOINT:
            phase = _CyclePhase.ACTIVE_CHECKPOINT
        elif event.kind is _ScenarioKind.INITIAL:
            phase = _CyclePhase.INITIAL
        elif event.kind is _ScenarioKind.OBSERVED:
            revision = _integer(fields, "revision")
            phase = (
                _CyclePhase.OBSERVED_CLOSE
                if revision == close
                else _CyclePhase.OBSERVED_REOPEN if revision == target else None
            )
        elif event.kind is _ScenarioKind.REQUEST_EDIT:
            phase = {
                "close-skylight": _CyclePhase.REQUEST_CLOSE,
                "reopen-skylight": _CyclePhase.REQUEST_REOPEN,
            }.get(fields.get("edit"))
        elif event.kind is _ScenarioKind.PREPARED and fields.get("kind") == "Terrain":
            revision = _integer(fields, "target_terrain_revision")
            phase = (
                _CyclePhase.PREPARED_CLOSE
                if revision == close
                else _CyclePhase.PREPARED_REOPEN if revision == target else None
            )
        elif event.kind is _ScenarioKind.LOCAL_RECOVERY:
            revision = _integer(fields, "geometry_revision")
            phase = (
                _CyclePhase.RECOVERY_CLOSE
                if revision == close
                else _CyclePhase.RECOVERY_REOPEN if revision == target else None
            )
        elif event.kind is _ScenarioKind.VISIBLE:
            phase = {
                "close-skylight": _CyclePhase.VISIBLE_CLOSE,
                "reopen-skylight": _CyclePhase.VISIBLE_REOPEN,
            }.get(fields.get("edit"))
        elif event.kind is _ScenarioKind.OBSOLETE_CANDIDATE:
            phase = _CyclePhase.OBSOLETE_CANDIDATE
        elif event.kind is _ScenarioKind.OBSOLETE_SKIPPED:
            phase = _CyclePhase.OBSOLETE_SKIPPED
        elif event.kind is _ScenarioKind.TRANSIENT_ARMED:
            phase = _CyclePhase.TRANSIENT_ARMED
        elif event.kind is _ScenarioKind.TRANSIENT_RECORDING:
            phase = _CyclePhase.TRANSIENT_RECORDING
        elif event.kind is _ScenarioKind.CAPTURE_SAVED:
            phase = _CyclePhase.CAPTURE_SAVED
        elif event.kind is _ScenarioKind.CAPTURE_COMPLETE:
            phase = _CyclePhase.CAPTURE_COMPLETE
        if phase is None:
            stream.reject(event)
        stream.event(phase, event)
    evidence = stream.finish()
    checkpoint_event = evidence[_CyclePhase.ACTIVE_CHECKPOINT]
    checkpoint = _CheckpointIdentity.from_event(
        action,
        checkpoint_event,
        "transient active checkpoint",
    )
    checkpoint.require_transient_active(checkpoint_event)
    checkpoint.require_ready(
        evidence[_CyclePhase.INITIAL],
        "terrain_revision",
        "transient initial ready",
    )
    prepared_close = evidence[_CyclePhase.PREPARED_CLOSE]
    candidate = evidence[_CyclePhase.OBSOLETE_CANDIDATE]
    skipped = evidence[_CyclePhase.OBSOLETE_SKIPPED]
    prepared_reopen = evidence[_CyclePhase.PREPARED_REOPEN]
    close_token = _integer(prepared_close.fields, "token_serial")
    reopen_token = _integer(prepared_reopen.fields, "token_serial")
    if reopen_token <= close_token:
        raise ScenarioLifecycleError("transient replacement token identity did not advance")
    for event, name, expected in (
        (prepared_close, "active_terrain_revision", initial),
        (prepared_close, "target_terrain_revision", close),
        (candidate, "terrain_revision", close),
        (candidate, "active_terrain_revision", initial),
        (candidate, "token_serial", close_token),
        (skipped, "token_serial", close_token),
        (skipped, "replacement_token_serial", reopen_token),
        (skipped, "replacement_terrain_revision", target),
        (prepared_reopen, "active_terrain_revision", initial),
        (prepared_reopen, "target_terrain_revision", target),
    ):
        _same_identity(event, name, expected, "transient")
    for phase in (_CyclePhase.TRANSIENT_ARMED, _CyclePhase.TRANSIENT_RECORDING):
        event = evidence[phase]
        for name, expected in (
            ("active_terrain_revision", initial),
            ("target_terrain_revision", target),
            ("staging_token_serial", reopen_token),
        ):
            _same_identity(event, name, expected, phase.value)
        _same_literal(event, "staging_stage", "Rebuilding", phase.value)
        _same_literal(event, "invalidation", "stale-active", phase.value)
        progress = event.fields.get("staging_progress", "")
        current, separator, total = progress.partition("/")
        if not separator or not current.isdigit() or not total.isdigit() or int(total) <= 0:
            raise ScenarioLifecycleError(f"{phase.value} identity has invalid progress")
    saved = evidence[_CyclePhase.CAPTURE_SAVED]
    checkpoint.require_saved(saved, "transient capture")
    if "invalidation_voxel_bound=Some((UVec3(0, 0, 0), UVec3(512, 512, 512)))" in text:
        raise ScenarioLifecycleError("transient identity invalidated the full DDGI domain")
    for event in events:
        if event.kind not in (_ScenarioKind.PROMOTION, _ScenarioKind.CONSUMER):
            continue
        name = (
            "geometry_revision"
            if event.kind is _ScenarioKind.PROMOTION
            else "geometry_revision"
        )
        if _integer(event.fields, name) > initial:
            raise ScenarioLifecycleError("transient identity exposed post-active publication")
    return {"active_revision": initial, "target_revision": target}


def _flora(action: ValidateScenarioLog, text: str) -> dict[str, int]:
    cycle = _validate_completed_cycle(
        action, text, mode="reopened", require_flora=True
    )
    return {"active_token": cycle.final_token, "final_revision": cycle.final_revision}


def validate_scenario_log(action: ValidateScenarioLog) -> dict[str, int]:
    text = action.console.read_text(encoding="utf-8", errors="replace")
    match action.validation:
        case ScenarioValidation.INFLIGHT_FINAL:
            return _validate_inflight(action, text)
        case ScenarioValidation.RADIANCE_STREAM:
            return validate_radiance_event_stream(text, action.spacing_voxels)
        case ScenarioValidation.DENSITY_STREAM:
            return validate_density_lifecycle(action.console)
        case ScenarioValidation.LOCAL_RECOVERY:
            return _validate_local(action, text)
        case ScenarioValidation.RUNTIME_FINAL:
            return _runtime_final(action, text)
        case ScenarioValidation.RUNTIME_TRANSIENT:
            return _runtime_transient(action, text)
        case ScenarioValidation.FLORA_CONSUMER:
            return _flora(action, text)
        case ScenarioValidation.TERRAIN_EDIT:
            return _validate_cycle(action, text)
    raise AssertionError(f"unhandled scenario validation: {action.validation}")


def validate_radiance_lifecycle(action: ValidateRadianceLifecycle) -> str:
    try:
        import analyze_environment_irradiance_capture as analyzer
    except ModuleNotFoundError:
        from scripts import analyze_environment_irradiance_capture as analyzer

    checkpoints = ("baseline", "r2-next-frame", "r4-next-frame", "final")

    def checkpoint_path(checkpoint: str) -> Path:
        if checkpoint == "final":
            return action.capture
        return action.capture.with_name(
            f"{action.capture.stem}.{checkpoint}{action.capture.suffix}"
        )

    paths = {checkpoint: checkpoint_path(checkpoint) for checkpoint in checkpoints}
    captures = {
        checkpoint: analyzer.load_capture(path) for checkpoint, path in paths.items()
    }
    identities = {}
    failures: list[str] = []
    for checkpoint, path in paths.items():
        identity_path = Path(f"{path}.identity.json")
        with identity_path.open(encoding="utf-8") as identity_file:
            identity = json.load(identity_file)
        identities[checkpoint] = identity
        capture = captures[checkpoint]
        if not analyzer.is_current_capture(capture):
            failures.append(f"{checkpoint}: capture is not current RFIRR")
        if not (
            capture.filter_evidence is not None
            and capture.grid_dimensions is not None
            and capture.configured_history_retention_q16 is not None
        ):
            failures.append(f"{checkpoint}: current DDGI filter proof is incomplete")
        if not analyzer.required_capture_planes_finite(capture):
            failures.append(
                f"{checkpoint}: required capture planes contain non-finite values"
            )
        if capture.spacing_voxels != action.spacing_voxels:
            failures.append(
                f"{checkpoint}: spacing is not {action.spacing_voxels}"
            )
        if identity.get("schema") != "re-flora-ddgi-radiance-capture-v1":
            failures.append(f"{checkpoint}: wrong identity schema")
        if identity.get("checkpoint") != checkpoint:
            failures.append(f"{checkpoint}: sidecar checkpoint mismatch")
        field = identity["active_field"]
        lifecycle_state = analyzer.LIFECYCLE_STATE_LABELS.get(
            capture.lifecycle_state
        )
        if not (
            field["field_serial"] == capture.field_serial
            and field["geometry_revision"] == capture.geometry_revision
            and field["radiance_revision"] == capture.radiance_revision
            and field["spacing_voxels"] == capture.spacing_voxels
            and str(field["lifecycle_state"]).lower() == lifecycle_state
            and field["update_epoch"] == capture.update_epoch
            and field["source_field_serial"] == capture.source_field_serial
            and field["source_radiance_revision"]
            == capture.source_radiance_revision
        ):
            failures.append(
                f"{checkpoint}: sidecar active field does not match v10 header"
            )

    baseline = identities["baseline"]
    r2 = identities["r2-next-frame"]
    r4 = identities["r4-next-frame"]
    final = identities["final"]
    baseline_revision = baseline["active_field"]["radiance_revision"]
    for checkpoint, identity in (("r2-next-frame", r2), ("r4-next-frame", r4)):
        if identity["capture_frame"] != identity["mutation_frame"] + 1:
            failures.append(
                f"{checkpoint}: capture is not the first rendered frame after mutation"
            )
        if identity["active_field"] != baseline["active_field"]:
            failures.append(
                f"{checkpoint}: old consumer-visible DDGI field changed"
            )
    baseline_sun = baseline["live_snapshot"]
    for checkpoint, changed in (
        ("r2-next-frame", r2["live_snapshot"]),
        ("r4-next-frame", r4["live_snapshot"]),
    ):
        for name in ("sun_direction", "sun_color", "sun_luminance"):
            if changed[name] == baseline_sun[name]:
                failures.append(f"{checkpoint}: {name} did not dynamically change")
    if not (
        r2["live_radiance_revision"] == baseline_revision + 1
        and r2["latest_radiance_revision"] == baseline_revision + 1
    ):
        failures.append("r2-next-frame: live/latest revision is not r2")
    if not (
        r2["building_field"]["radiance_revision"] == baseline_revision + 1
        and r2["builder_latched_radiance_revision"] == baseline_revision + 1
        and r2["builder_latched_snapshot"] == r2["live_snapshot"]
    ):
        failures.append("r2-next-frame: builder did not latch the exact r2 snapshot")
    if not (
        r4["live_radiance_revision"] == baseline_revision + 3
        and r4["latest_radiance_revision"] == baseline_revision + 3
    ):
        failures.append("r4-next-frame: latest-wins revision is not r4")
    if not (
        r4["building_field"] == r2["building_field"]
        and r4["builder_latched_radiance_revision"] == baseline_revision + 1
        and r4["builder_latched_snapshot"] == r2["live_snapshot"]
    ):
        failures.append(
            "r4-next-frame: in-flight r2 identity or latched snapshot mutated"
        )
    final_active = final["active_field"]
    r2_building = r2["building_field"]
    if not (
        final["live_radiance_revision"] == baseline_revision + 3
        and final["latest_radiance_revision"] == baseline_revision + 3
        and final_active["radiance_revision"] == baseline_revision + 3
    ):
        failures.append("final: latest r4 is not consumer-active")
    if not (
        final_active["source_field_serial"] == r2_building["field_serial"]
        and final_active["field_serial"] == r2_building["field_serial"] + 1
    ):
        failures.append("final: r3 allocated a field or r4 did not consume r2")

    frame_comparisons = {}
    direct_comparisons = {}
    for checkpoint in ("r2-next-frame", "r4-next-frame"):
        frame = analyzer.compare_radiance_frame(
            captures[checkpoint], captures["baseline"]
        )
        direct = analyzer.compare_direct_light_baseline(
            captures[checkpoint], captures["baseline"], action.sunlit_roi
        )
        frame_comparisons[checkpoint] = frame
        direct_comparisons[checkpoint] = direct
        if not frame["compatible"]:
            failures.append(f"{checkpoint}: field metadata changed")
        if not frame["environment_payload_bit_exact"]:
            failures.append(f"{checkpoint}: old DDGI irradiance payload changed")
        if not (
            frame["world_xyz_bit_exact"]
            and frame["terrain_hit_mask_bit_exact"]
        ):
            failures.append(
                f"{checkpoint}: world XYZ or terrain hit mask changed"
            )
        if not (
            direct["compatible"]
            and direct["sunlit_roi_luminance_absolute_delta"]
            >= action.minimum_direct_light_delta
        ):
            failures.append(
                f"{checkpoint}: direct-light ROI delta is below "
                f"{action.minimum_direct_light_delta:g}"
            )
    report = {
        "base_capture": str(action.capture),
        "spacing_voxels": action.spacing_voxels,
        "sunlit_roi": list(action.sunlit_roi),
        "min_direct_delta": action.minimum_direct_light_delta,
        "frame_comparisons": frame_comparisons,
        "direct_comparisons": direct_comparisons,
        "identities": identities,
        "validation_failures": failures,
    }
    if failures:
        raise ValueError("; ".join(str(failure) for failure in failures))
    return json.dumps(report, indent=2, sort_keys=True) + "\n"


def require_current_capture(capture, checkpoint: str, failures: list[str]) -> None:
    try:
        import analyze_environment_irradiance_capture as analyzer
    except ModuleNotFoundError:
        from scripts import analyze_environment_irradiance_capture as analyzer

    if not analyzer.is_current_capture(capture):
        failures.append(f"{checkpoint}: capture is not current RFIRR")
    if not (
        capture.filter_evidence is not None
        and capture.grid_dimensions is not None
        and capture.configured_history_retention_q16 is not None
    ):
        failures.append(f"{checkpoint}: current DDGI filter proof is incomplete")


def require_required_planes_finite(
    capture, checkpoint: str, failures: list[str]
) -> None:
    try:
        import analyze_environment_irradiance_capture as analyzer
    except ModuleNotFoundError:
        from scripts import analyze_environment_irradiance_capture as analyzer

    if not analyzer.required_capture_planes_finite(capture):
        failures.append(
            f"{checkpoint}: required capture planes contain non-finite values"
        )
