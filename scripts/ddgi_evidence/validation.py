from __future__ import annotations

import json
import re
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import NoReturn

from runtime_log_diagnostics import first_fatal_diagnostic

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


def _last_match(pattern: str, text: str, label: str) -> re.Match[str]:
    matches = list(re.finditer(pattern, text, re.MULTILINE))
    if not matches:
        raise ValueError(f"missing {label}")
    return matches[-1]


def _require_markers(text: str, markers: tuple[str, ...]) -> None:
    missing = [marker for marker in markers if marker not in text]
    if missing:
        raise ValueError("missing markers: " + ", ".join(missing))


def _initial_revision(text: str, state: str = "") -> int:
    if state == "initial-open":
        match = _last_match(
            r"\[ENV_LIGHT_TEST\] ready case=portal backend=ddgi terrain_revision=(\d+)",
            text,
            "initial portal terrain revision",
        )
    else:
        match = _last_match(
            r"\[ENV_LIGHT_EDIT_CYCLE\] initial probe field ready terrain_revision=(\d+)",
            text,
            "initial terrain revision",
        )
    return int(match.group(1))


def _validate_inflight(text: str, spacing: int) -> dict[str, int]:
    initial = _initial_revision(text)
    obsolete, replacement = initial + 1, initial + 2
    _require_markers(
        text,
        (
            f"initial probe field ready terrain_revision={initial}",
            f"requested edit=close-skylight source_revision={initial} target_revision={obsolete}",
            f"visible terrain publication complete edit=close-skylight target_revision={obsolete}",
            f"obsolete candidate observed terrain_revision={obsolete}",
            f"requested edit=reopen-skylight source_revision={obsolete} target_revision={replacement}",
            f"visible terrain publication complete edit=reopen-skylight target_revision={replacement}",
            "[DDGI] obsolete staging promotion skipped",
            f"replacement_terrain_revision={replacement}",
            "[DDGI] staging promoted",
            "kind=Terrain",
            f"edited probe field ready edit=reopen-skylight terrain_revision={replacement}",
            f"complete mode=reopened final_terrain_revision={replacement}",
            "[ENV_IRRADIANCE_CAPTURE] saved",
        ),
    )
    if re.search(
        rf"\[DDGI\] staging promoted .*kind=Terrain.*terrain_revision={obsolete}(?:\D|$)",
        text,
    ):
        raise ValueError(f"obsolete terrain revision {obsolete} became active")
    return {"final_revision": replacement, "spacing_voxels": spacing}


def _validate_cycle(text: str, spacing: int, mode: str) -> dict[str, int]:
    initial = _initial_revision(text)
    closed, reopened = initial + 1, initial + 2
    markers = [
        f"initial probe field ready terrain_revision={initial}",
        f"requested edit=close-skylight source_revision={initial} target_revision={closed}",
        f"visible terrain publication complete edit=close-skylight target_revision={closed}",
        f"edited probe field ready edit=close-skylight terrain_revision={closed}",
        "[ENV_IRRADIANCE_CAPTURE] saved",
    ]
    if mode == "closed":
        markers.append(f"complete mode=closed final_terrain_revision={closed}")
        final = closed
    else:
        markers.extend(
            (
                f"requested edit=reopen-skylight source_revision={closed} target_revision={reopened}",
                f"visible terrain publication complete edit=reopen-skylight target_revision={reopened}",
                f"edited probe field ready edit=reopen-skylight terrain_revision={reopened}",
                f"requested density rebuild terrain_revision={reopened} spacing_voxels={spacing}",
                f"density rebuild ready terrain_revision={reopened}",
                f"complete mode=reopened final_terrain_revision={reopened}",
            )
        )
        final = reopened
    _require_markers(text, tuple(markers))
    return {"final_revision": final}


def _validate_local(action: ValidateScenarioLog, text: str) -> dict[str, int]:
    initial = _initial_revision(text)
    final = initial + 1
    if re.search(
        rf"runtime observed visible terrain revision={final}(?:\D|$).*"
        r"invalidation_voxel_bound=Some\(\(UVec3\(0, 0, 0\), UVec3\(512, 512, 512\)\)\)",
        text,
    ):
        raise ValueError("terrain edit invalidated the full DDGI domain")
    recovery = _last_match(
        rf"^.*\[DDGI\]\[LOCAL_RECOVERY\] prepared .*geometry_revision={final}(?:\D|$).*$",
        text,
        "local recovery partition",
    ).group(0)
    recovery_fields = _fields(recovery)
    dirty = _integer(recovery_fields, "dirty_probes")
    preserved = _integer(recovery_fields, "preserved_probes")
    if dirty == 0 or preserved == 0:
        raise ValueError("local recovery requires nonempty dirty and preserved partitions")
    promotion_match = _last_match(
        rf"^.*\[DDGI\] staging promoted .*geometry_revision={final}(?:\D|$).*$",
        text,
        "terrain promotion",
    )
    promotion = promotion_match.group(0)
    if "published_source=Some(" not in promotion:
        raise ValueError("terrain promotion lacks an explicit history source")
    epoch = _integer(_fields(promotion), "published_update_epoch")
    if epoch < action.minimum_epoch:
        raise ValueError(f"terrain promotion epoch {epoch} is below {action.minimum_epoch}")
    high_delta = 0
    for line in text[promotion_match.start() :].splitlines():
        if (
            "[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated" in line
            and f"geometry_revision={final}" in line
            and float(_fields(line).get("max_abs_rgb_delta", "0")) > 0.1
        ):
            high_delta += 1
    if high_delta > action.maximum_high_delta_epochs:
        raise ValueError(
            f"post-promotion high-delta epochs {high_delta} exceed "
            f"{action.maximum_high_delta_epochs}"
        )
    return {
        "final_revision": final,
        "dirty_probes": dirty,
        "preserved_probes": preserved,
        "promoted_epoch": epoch,
        "post_promotion_high_delta_epochs": high_delta,
    }


def _runtime_final(action: ValidateScenarioLog, text: str) -> dict[str, int]:
    initial = _initial_revision(text, action.state)
    final = {
        "initial-open": initial,
        "closed": initial + 1,
        "sequential-reopened": initial + 2,
        "inflight-latest-wins": initial + 2,
    }[action.state]
    if action.state == "initial-open":
        markers = (
            f"ready case=portal backend=ddgi terrain_revision={initial} geometry=static",
            "[ENV_IRRADIANCE_CAPTURE] saved",
            "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run",
        )
    else:
        closed = initial + 1
        markers = [
            f"initial probe field ready terrain_revision={initial}",
            f"requested edit=close-skylight source_revision={initial} target_revision={closed}",
            "invalidation_voxel_bound=Some((UVec3(",
            f"target_terrain_revision={final}",
            "[DDGI] staging promoted",
            f"terrain_revision={final}",
            "[DDGI][CONSUMERS] consumer_set=terrain_compute,flora_raster",
            "active_token_serial=",
            "[ENV_IRRADIANCE_CAPTURE] saved",
            "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run",
        ]
        if action.state == "closed":
            markers.append(f"complete mode=closed final_terrain_revision={final}")
        else:
            markers.extend(
                (
                    f"requested edit=reopen-skylight source_revision={closed} target_revision={final}",
                    f"complete mode=reopened final_terrain_revision={final}",
                )
            )
        if action.state == "inflight-latest-wins":
            markers.extend(
                (
                    f"obsolete candidate observed terrain_revision={closed}",
                    "[DDGI] obsolete staging promotion skipped",
                    f"replacement_terrain_revision={final}",
                )
            )
    _require_markers(text, tuple(markers))
    if action.state != "initial-open":
        if "invalidation_voxel_bound=Some((UVec3(0, 0, 0), UVec3(512, 512, 512)))" in text:
            raise ValueError("full-domain DDGI invalidation returned")
        consumer = _last_match(
            rf"^.*\[DDGI\]\[CONSUMERS\] consumer_set=terrain_compute,flora_raster "
            rf".*geometry_revision={final}(?:\D|$).*state=Converging.*$",
            text,
            "shared consumer publication",
        ).group(0)
        if _integer(_fields(consumer), "update_epoch") < action.minimum_epoch:
            raise ValueError("shared consumer exposed insufficient local recovery")
        promotion = _last_match(
            rf"^.*\[DDGI\] staging promoted .*kind=Terrain .*geometry_revision={final}"
            rf"(?:\D|$).*published_state=Converging.*$",
            text,
            "terrain promotion",
        ).group(0)
        if _integer(_fields(promotion), "published_update_epoch") < action.minimum_epoch:
            raise ValueError("terrain candidate promoted before local recovery")
        if "published_source=Some(" not in promotion:
            raise ValueError("terrain promotion did not retain resident history")
        if action.state == "inflight-latest-wins" and re.search(
            rf"\[DDGI\] staging promoted .*kind=Terrain.*geometry_revision={initial + 1}(?:\D|$)",
            text,
        ):
            raise ValueError("obsolete terrain revision promoted")
    return {"final_revision": final}


def _runtime_transient(text: str) -> dict[str, int]:
    _require_markers(
        text,
        (
            "[ENV_LIGHT_EDIT_CYCLE] initial probe field ready terrain_revision=",
            "[ENV_LIGHT_EDIT_INFLIGHT] obsolete candidate observed terrain_revision=",
            "[ENV_LIGHT_EDIT_CYCLE] requested edit=reopen-skylight source_revision=",
            "[DDGI] obsolete staging promotion skipped",
            "invalidation_voxel_bound=Some((UVec3(",
            "coordinator=BuildingTerrain",
            "invalidation=stale-active",
            "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] armed active_terrain_revision=Some(",
            "[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE] recording active_terrain_revision=Some(",
            "staging_token_serial=Some(",
            "staging_stage=Rebuilding",
            "[ENV_IRRADIANCE_CAPTURE] saved",
            "[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run",
        ),
    )
    if "invalidation_voxel_bound=Some((UVec3(0, 0, 0), UVec3(512, 512, 512)))" in text:
        raise ValueError("transient state invalidated the full DDGI domain")
    if not re.search(
        r"\[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE\] recording .*staging_progress=\d+/[1-9]\d* "
        r".*coordinator=BuildingTerrain",
        text,
    ):
        raise ValueError("missing GPU-visible staging progress")
    armed = _last_match(
        r"^.*\[ENV_LIGHT_EDIT_INFLIGHT_CAPTURE\] armed.*$", text, "armed transient capture"
    ).group(0)
    active_match = re.search(r"active_terrain_revision=Some\((\d+)\)", armed)
    target_match = re.search(r"target_terrain_revision=(\d+)", armed)
    if not active_match or not target_match:
        raise ValueError("transient capture lacks active/target revisions")
    active, target = int(active_match.group(1)), int(target_match.group(1))
    if active == target:
        raise ValueError("transient capture active and target revisions are equal")
    for pattern, label in (
        (r"\[DDGI\] staging promoted .*kind=Terrain.*geometry_revision=(\d+)", "promotion"),
        (r"\[DDGI\]\[CONSUMERS\].*geometry_revision=(\d+)", "consumer"),
    ):
        if any(int(match.group(1)) > active for match in re.finditer(pattern, text)):
            raise ValueError(f"transient capture exposed post-active {label}")
    obsolete = _last_match(
        r"^.*\[DDGI\] obsolete staging promotion skipped .*coordinator=.*$",
        text,
        "obsolete staging completion",
    )
    latest = _last_match(
        r"^.*\[DDGI\] staging prepared .*target_terrain_revision=.*$",
        text,
        "latest staging start",
    )
    if latest.start() <= obsolete.start():
        raise ValueError("terrain staging updates overlapped")
    return {"active_revision": active, "target_revision": target}


def _flora(text: str) -> dict[str, int]:
    final = _initial_revision(text) + 2
    consumer = _last_match(
        rf"^.*\[DDGI\]\[CONSUMERS\].*geometry_revision={final}(?:\D|$).*$",
        text,
        "final flora consumer publication",
    ).group(0)
    token = _integer(_fields(consumer), "active_token_serial")
    if not re.search(
        rf"\[DDGI\]\[FLORA_CONSUMER\] draw_recorded active_token_serial={token} "
        rf"terrain_revision={final}(?:\D|$).*instance_count=[1-9]\d*",
        text,
    ):
        raise ValueError("flora draw did not consume the final DDGI publication")
    return {"active_token": token, "final_revision": final}


def validate_scenario_log(action: ValidateScenarioLog) -> dict[str, int]:
    text = action.console.read_text(encoding="utf-8", errors="replace")
    match action.validation:
        case ScenarioValidation.INFLIGHT_FINAL:
            return _validate_inflight(text, action.spacing_voxels)
        case ScenarioValidation.RADIANCE_STREAM:
            return validate_radiance_event_stream(text, action.spacing_voxels)
        case ScenarioValidation.DENSITY_STREAM:
            from validate_ddgi_density_lifecycle import validate_density_lifecycle

            return validate_density_lifecycle(action.console)
        case ScenarioValidation.LOCAL_RECOVERY:
            return _validate_local(action, text)
        case ScenarioValidation.RUNTIME_FINAL:
            return _runtime_final(action, text)
        case ScenarioValidation.RUNTIME_TRANSIENT:
            return _runtime_transient(text)
        case ScenarioValidation.FLORA_CONSUMER:
            return _flora(text)
        case ScenarioValidation.TERRAIN_EDIT:
            return _validate_cycle(text, action.spacing_voxels, action.state)
    raise AssertionError(f"unhandled scenario validation: {action.validation}")


def validate_radiance_lifecycle(action: ValidateRadianceLifecycle) -> str:
    from validate_ddgi_radiance_lifecycle import validate

    report = validate(
        action.capture,
        action.spacing_voxels,
        action.sunlit_roi,
        action.minimum_direct_light_delta,
    )
    failures = report["validation_failures"]
    if failures:
        raise ValueError("; ".join(str(failure) for failure in failures))
    return json.dumps(report, indent=2, sort_keys=True) + "\n"
