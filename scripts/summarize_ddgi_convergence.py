#!/usr/bin/env python3
"""Validate DDGI temporal epoch curves and emit machine-readable provenance."""

from __future__ import annotations

import argparse
import json
import math
import re
import struct
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path


DEFAULT_CASES = ("sealed", "portal", "donor", "dogleg")
DEFAULT_SPACINGS = (32, 16)
CONTRACT_PATH = Path(__file__).resolve().parents[1] / "config/ddgi_convergence_acceptance.toml"
EVIDENCE_MARKER = "[DDGI_CONVERGENCE_EVIDENCE]"
VALIDATION_MARKER = "[DDGI_CONVERGENCE_EVIDENCE] full-atlas validated"
TERMINAL_MARKER = "[DDGI_CONVERGENCE_EVIDENCE] terminal"
LOG_TIME_PATTERN = r"(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d\.\d{3}"
EVIDENCE_LOG_PREFIX = (
    rf"^\[(?P<log_time>{LOG_TIME_PATTERN}) DEBUG "
    r"re_flora::ddgi::runtime::convergence_evidence\] "
)
VALIDATION_PATTERN = re.compile(
    EVIDENCE_LOG_PREFIX
    + re.escape(VALIDATION_MARKER)
    + r" field_serial=(?P<field_serial>\d+) "
    r"source_field_serial=(?P<source>none|\d+) geometry_revision=(?P<geometry>\d+) "
    r"radiance_revision=(?P<radiance>\d+) "
    r"spacing_voxels=(?P<spacing>\d+) "
    r"state=(?P<state>Converging|Converged) update_epoch=(?P<epoch>\d+) "
    r"max_abs_rgb_delta=(?P<absolute>[0-9.eE+-]+) "
    r"max_rel_rgb_delta=(?P<relative>[0-9.eE+-]+) "
    r"non_finite=(?P<nonfinite>\d+) "
    r"negative_rgb_texels=(?P<negative>\d+) "
    r"valid_texels=(?P<valid>\d+) "
    r"scanned_stored_texels=(?P<scanned>\d+) "
    r"abs_threshold=(?P<absolute_threshold>[0-9.eE+-]+) "
    r"rel_threshold=(?P<relative_threshold>[0-9.eE+-]+) "
    r"consecutive_below=(?P<consecutive>\d+)/(?P<required>\d+)$"
)
TERMINAL_PATTERN = re.compile(
    EVIDENCE_LOG_PREFIX
    + re.escape(TERMINAL_MARKER)
    + r" field_serial=(?P<field_serial>\d+) geometry_revision=(?P<geometry>\d+) "
    r"radiance_revision=(?P<radiance>\d+) "
    r"spacing_voxels=(?P<spacing>\d+) "
    r"update_epoch=(?P<epoch>\d+) reason=(?P<reason>Threshold|SampleBudget)$"
)
POLICY_PATTERN = re.compile(
    rf"^\[(?P<log_time>{LOG_TIME_PATTERN}) "
    r"INFO re_flora::tracer\] \[DDGI\] initialization requested "
    r"terrain_revision=(?P<terrain>\d+) "
    r"spacing_voxels=(?P<policy_spacing>\d+) "
    r"probes=(?P<probes>\d+) stage=(?P<stage>RelocationPending) "
    r"convergence_max_absolute_rgb_delta=(?P<absolute>[0-9.eE+-]+) "
    r"convergence_max_relative_rgb_delta=(?P<relative>[0-9.eE+-]+) "
    r"convergence_relative_floor=(?P<relative_floor>[0-9.eE+-]+) "
    r"convergence_consecutive_epochs=(?P<consecutive>\d+) "
    r"convergence_minimum_update_epochs=(?P<minimum>\d+) "
    r"convergence_maximum_update_epochs=(?P<maximum>\d+)$",
    re.MULTILINE,
)
RELOCATION_POPULATION_PATTERN = re.compile(
    rf"^\[(?P<log_time>{LOG_TIME_PATTERN}) "
    r"INFO re_flora::tracer\] \[DDGI\] relocation stats "
    r"probes=(?P<total>\d+) valid=(?P<valid>\d+) failed=(?P<invalid>\d+) "
    r".*$",
    re.MULTILINE,
)


@dataclass(frozen=True)
class Policy:
    absolute_threshold: float
    relative_threshold: float
    relative_floor: float
    consecutive_epochs: int
    minimum_epoch_count: int
    maximum_update_epoch: int


@dataclass(frozen=True)
class InitializationEvent:
    log_time: str
    terrain_revision: int
    spacing_voxels: int
    probe_count: int
    stage: str
    policy: Policy


@dataclass(frozen=True)
class ProbePopulation:
    total_count: int
    valid_count: int
    invalid_count: int

    @classmethod
    def from_match(cls, match: re.Match[str], source: Path) -> ProbePopulation:
        population = cls(
            total_count=int(match.group("total")),
            valid_count=int(match.group("valid")),
            invalid_count=int(match.group("invalid")),
        )
        for field, count in (
            ("total_probe_count", population.total_count),
            ("valid_probe_count", population.valid_count),
            ("invalid_probe_count", population.invalid_count),
        ):
            require_rust_unsigned(count, "u32", field, source)
        if population.total_count == 0 or population.valid_count == 0:
            raise ValueError(
                f"DDGI relocation population in {source} must contain total and valid probes"
            )
        if population.total_count != population.valid_count + population.invalid_count:
            raise ValueError(
                f"DDGI relocation population in {source} is not a complete partition: "
                f"total={population.total_count} valid={population.valid_count} "
                f"invalid={population.invalid_count}"
            )
        return population


@dataclass(frozen=True)
class AtlasTexelLayout:
    interior_texels_per_valid_probe: int
    stored_texels_per_valid_probe: int

    @classmethod
    def load(cls, path: Path) -> AtlasTexelLayout:
        contract = tomllib.loads(path.read_text())
        layout = contract.get("atlas_texel_layout")
        expected_fields = {
            "interior_texels_per_valid_probe",
            "stored_texels_per_valid_probe",
        }
        if not isinstance(layout, dict) or set(layout) != expected_fields:
            raise ValueError("invalid DDGI convergence atlas texel layout contract")
        interior = layout["interior_texels_per_valid_probe"]
        stored = layout["stored_texels_per_valid_probe"]
        if (
            type(interior) is not int
            or type(stored) is not int
            or not 0 < interior <= stored <= (1 << 32) - 1
        ):
            raise ValueError("invalid DDGI convergence atlas texel layout dimensions")
        return cls(interior, stored)


@dataclass(frozen=True)
class ValidatedAtlasCoverage:
    interior_texel_count: int
    stored_texel_count: int

    @classmethod
    def from_record(cls, record: dict[str, object]) -> ValidatedAtlasCoverage:
        return cls(
            interior_texel_count=int(record["valid_texel_count"]),
            stored_texel_count=int(record["scanned_stored_texel_count"]),
        )

    def require_complete(
        self,
        population: ProbePopulation,
        layout: AtlasTexelLayout,
        source: Path,
    ) -> None:
        expected = ValidatedAtlasCoverage(
            interior_texel_count=(
                population.valid_count * layout.interior_texels_per_valid_probe
            ),
            stored_texel_count=(
                population.valid_count * layout.stored_texels_per_valid_probe
            ),
        )
        if self != expected:
            raise ValueError(
                f"validated atlas record in {source} has incomplete coverage of the "
                f"valid-probe atlas: "
                f"population=total:{population.total_count},valid:{population.valid_count},"
                f"invalid:{population.invalid_count} "
                f"interior={self.interior_texel_count}/{expected.interior_texel_count} "
                f"stored={self.stored_texel_count}/{expected.stored_texel_count}"
            )


@dataclass(frozen=True)
class ConvergenceProvenance:
    initialization: InitializationEvent
    probe_population: ProbePopulation
    atlas_layout: AtlasTexelLayout


@dataclass(frozen=True)
class TerminalIdentity:
    log_time: str
    field_serial: int
    geometry_revision: int
    radiance_revision: int
    spacing_voxels: int
    update_epoch: int
    reason: str


@dataclass(frozen=True)
class ValidationWireContract:
    decimal_places: int
    integer_types: dict[str, str]
    optional_integer_types: dict[str, str]
    float_types: dict[str, str]


def rust_f32(value: float) -> float:
    return struct.unpack("!f", struct.pack("!f", value))[0]


def possible_metric_below_threshold(
    displayed: float, threshold: float, decimal_places: int
) -> set[bool]:
    half_rounding_cell = 0.5 * 10.0 ** (-decimal_places)
    lower = displayed - half_rounding_cell
    upper = displayed + half_rounding_cell
    rust_threshold = rust_f32(threshold)
    possible: set[bool] = set()
    if lower <= rust_threshold:
        possible.add(True)
    if upper > rust_threshold:
        possible.add(False)
    return possible


def possible_below_threshold(
    record: dict[str, object], policy: Policy, decimal_places: int
) -> set[bool]:
    absolute = possible_metric_below_threshold(
        float(record["max_absolute_rgb_delta"]),
        policy.absolute_threshold,
        decimal_places,
    )
    relative = possible_metric_below_threshold(
        float(record["max_relative_rgb_delta"]),
        policy.relative_threshold,
        decimal_places,
    )
    possible: set[bool] = set()
    if True in absolute and True in relative:
        possible.add(True)
    if False in absolute or False in relative:
        possible.add(False)
    return possible


def validation_float_token(value: float, decimal_places: int) -> str:
    return f"{rust_f32(value):.{decimal_places}f}"


def canonical_validation_line(
    record: dict[str, object], policy: Policy, wire: ValidationWireContract
) -> str:
    source = record["source_field_serial"]
    source_token = "none" if source is None else str(source)
    return (
        f"{VALIDATION_MARKER} field_serial={record['field_serial']} "
        f"source_field_serial={source_token} "
        f"geometry_revision={record['geometry_revision']} "
        f"radiance_revision={record['radiance_revision']} "
        f"spacing_voxels={record['spacing_voxels']} state={record['state']} "
        f"update_epoch={record['update_epoch']} "
        f"max_abs_rgb_delta={validation_float_token(float(record['max_absolute_rgb_delta']), wire.decimal_places)} "
        f"max_rel_rgb_delta={validation_float_token(float(record['max_relative_rgb_delta']), wire.decimal_places)} "
        f"non_finite={record['nonfinite_count']} "
        f"negative_rgb_texels={record['negative_rgb_texel_count']} "
        f"valid_texels={record['valid_texel_count']} "
        f"scanned_stored_texels={record['scanned_stored_texel_count']} "
        f"abs_threshold={validation_float_token(policy.absolute_threshold, wire.decimal_places)} "
        f"rel_threshold={validation_float_token(policy.relative_threshold, wire.decimal_places)} "
        f"consecutive_below={record['consecutive_below_threshold']}/"
        f"{record['required_consecutive_epochs']}"
    )


def canonical_terminal_line(terminal: TerminalIdentity) -> str:
    return (
        f"{TERMINAL_MARKER} field_serial={terminal.field_serial} "
        f"geometry_revision={terminal.geometry_revision} "
        f"radiance_revision={terminal.radiance_revision} "
        f"spacing_voxels={terminal.spacing_voxels} "
        f"update_epoch={terminal.update_epoch} reason={terminal.reason}"
    )


def canonical_policy_suffix(contract_path: Path) -> str:
    source = contract_path.read_text().split("[", 1)[0]

    def token(field: str) -> str:
        matches = re.findall(rf"(?m)^{re.escape(field)}\s*=\s*(\S+)\s*$", source)
        if len(matches) != 1:
            raise ValueError(
                f"expected exactly one {field} token in convergence contract"
            )
        return matches[0]

    return (
        "convergence_max_absolute_rgb_delta="
        f"{token('absolute_threshold')} "
        "convergence_max_relative_rgb_delta="
        f"{token('relative_threshold')} "
        f"convergence_relative_floor={token('relative_floor')} "
        f"convergence_consecutive_epochs={token('consecutive_epochs')} "
        f"convergence_minimum_update_epochs={token('minimum_update_epochs')} "
        f"convergence_maximum_update_epochs={token('maximum_update_epochs')}"
    )


def canonical_policy_line(match: re.Match[str], contract_path: Path) -> str:
    return (
        f"[{match.group('log_time')} INFO re_flora::tracer] "
        "[DDGI] initialization requested "
        f"terrain_revision={int(match.group('terrain'))} "
        f"spacing_voxels={int(match.group('policy_spacing'))} "
        f"probes={int(match.group('probes'))} "
        f"stage={match.group('stage')} "
        f"{canonical_policy_suffix(contract_path)}"
    )


def load_validation_wire_contract(path: Path) -> ValidationWireContract:
    contract = tomllib.loads(path.read_text())
    wire = contract.get("validation_wire")
    if not isinstance(wire, dict):
        raise ValueError("missing DDGI convergence validation wire contract")
    integer_types = wire.get("integer_types")
    optional_integer_types = wire.get("optional_integer_types")
    float_types = wire.get("float_types")
    decimal_places = wire.get("decimal_places")
    if (
        not isinstance(integer_types, dict)
        or not isinstance(optional_integer_types, dict)
        or not isinstance(float_types, dict)
    ):
        raise ValueError("invalid DDGI convergence validation wire contract")
    if (
        not isinstance(decimal_places, int)
        or isinstance(decimal_places, bool)
        or decimal_places <= 0
    ):
        raise ValueError("invalid DDGI convergence validation decimal precision")
    if any(type_name not in ("u32", "u64") for type_name in integer_types.values()):
        raise ValueError("unsupported DDGI convergence integer wire type")
    if any(
        type_name not in ("u32", "u64")
        for type_name in optional_integer_types.values()
    ):
        raise ValueError("unsupported DDGI convergence optional integer wire type")
    if any(type_name != "f32" for type_name in float_types.values()):
        raise ValueError("unsupported DDGI convergence float wire type")
    field_sets = (
        set(integer_types),
        set(optional_integer_types),
        set(float_types),
    )
    if any(
        left & right
        for index, left in enumerate(field_sets)
        for right in field_sets[index + 1 :]
    ):
        raise ValueError("DDGI convergence validation wire field sets must be disjoint")
    return ValidationWireContract(
        decimal_places,
        dict(integer_types),
        dict(optional_integer_types),
        dict(float_types),
    )


def load_acceptance_contract(path: Path) -> Policy:
    contract = tomllib.loads(path.read_text())
    if contract.get("schema_version") != 1:
        raise ValueError("unsupported DDGI convergence acceptance contract")
    float_fields = (
        "absolute_threshold",
        "relative_threshold",
        "relative_floor",
    )
    integer_fields = (
        "consecutive_epochs",
        "minimum_update_epochs",
        "maximum_update_epochs",
        "terminal_update_epoch",
    )
    if any(
        not isinstance(contract.get(field), (int, float))
        or isinstance(contract.get(field), bool)
        or not math.isfinite(float(contract[field]))
        for field in float_fields
    ) or any(
        not isinstance(contract.get(field), int)
        or isinstance(contract.get(field), bool)
        for field in integer_fields
    ):
        raise ValueError("invalid DDGI convergence acceptance policy contract")
    maximum_update_epochs = int(contract["maximum_update_epochs"])
    terminal_update_epoch = int(contract["terminal_update_epoch"])
    if maximum_update_epochs <= 0 or terminal_update_epoch != maximum_update_epochs - 1:
        raise ValueError("invalid DDGI convergence acceptance epoch contract")
    return Policy(
        float(contract["absolute_threshold"]),
        float(contract["relative_threshold"]),
        float(contract["relative_floor"]),
        int(contract["consecutive_epochs"]),
        int(contract["minimum_update_epochs"]),
        terminal_update_epoch,
    )


def expected_initialization_probe_count(path: Path, spacing_voxels: int) -> int:
    contract = tomllib.loads(path.read_text())
    grid = contract.get("initialization_grid")
    if not isinstance(grid, dict) or set(grid) != {"world_extent_voxels"}:
        raise ValueError("invalid DDGI convergence initialization grid contract")
    world_extent = grid["world_extent_voxels"]
    if (
        type(world_extent) is not int
        or not 0 < world_extent <= (1 << 32) - 1
        or world_extent % spacing_voxels != 0
    ):
        raise ValueError("invalid DDGI convergence initialization world extent")
    side = world_extent // spacing_voxels + 1
    probe_count = side * side * side
    if probe_count > (1 << 32) - 1:
        raise ValueError("DDGI convergence initialization probe count exceeds u32")
    return probe_count


def require_policy_matches_contract(policy: Policy, contract: Policy) -> None:
    for field in ("absolute_threshold", "relative_threshold", "relative_floor"):
        runtime_value = float(getattr(policy, field))
        contract_value = float(getattr(contract, field))
        try:
            matches = rust_f32(runtime_value) == rust_f32(contract_value)
        except OverflowError as error:
            raise ValueError(
                f"acceptance convergence {field} is not representable as Rust f32"
            ) from error
        if not matches:
            raise ValueError(
                f"runtime convergence {field} drifted from acceptance contract: "
                f"runtime={runtime_value} contract={contract_value}"
            )
    for field in (
        "consecutive_epochs",
        "minimum_epoch_count",
        "maximum_update_epoch",
    ):
        runtime_value = int(getattr(policy, field))
        contract_value = int(getattr(contract, field))
        if runtime_value != contract_value:
            raise ValueError(
                f"runtime convergence {field} drifted from acceptance contract: "
                f"runtime={runtime_value} contract={contract_value}"
            )


def require_rust_unsigned(value: int, type_name: str, field: str, source: Path) -> None:
    width = int(type_name.removeprefix("u"))
    if value < 0 or value > (1 << width) - 1:
        raise ValueError(
            f"{field}={value} in {source} exceeds Rust wire type {type_name}"
        )


def require_nonnegative_f32(value: float, field: str, source: Path) -> None:
    try:
        rounded = struct.unpack("!f", struct.pack("!f", value))[0]
    except OverflowError as error:
        raise ValueError(
            f"{field}={value} in {source} exceeds finite Rust wire type f32"
        ) from error
    if not math.isfinite(value) or not math.isfinite(rounded) or value < 0.0:
        raise ValueError(
            f"{field}={value} in {source} is not a nonnegative finite Rust f32"
        )


def require_policy_wire_legality(policy: Policy, maximum_epochs: int, source: Path) -> None:
    for field in ("absolute_threshold", "relative_threshold", "relative_floor"):
        require_nonnegative_f32(float(getattr(policy, field)), field, source)
    require_rust_unsigned(policy.consecutive_epochs, "u32", "consecutive_epochs", source)
    require_rust_unsigned(policy.minimum_epoch_count, "u32", "minimum_update_epochs", source)
    require_rust_unsigned(maximum_epochs, "u32", "maximum_update_epochs", source)
    if (
        policy.consecutive_epochs == 0
        or policy.minimum_epoch_count == 0
        or maximum_epochs == 0
        or policy.minimum_epoch_count > maximum_epochs
    ):
        raise ValueError(f"invalid DDGI convergence runtime policy in {source}")


def require_global_validation_legality(
    records: list[dict[str, object]],
    evidence_path: Path,
    wire: ValidationWireContract,
    policy: Policy,
    provenance: ConvergenceProvenance,
) -> list[list[dict[str, object]]]:
    for record in records:
        numeric_fields = set(record) - {"state", "log_time"}
        contracted_fields = (
            set(wire.integer_types)
            | set(wire.optional_integer_types)
            | set(wire.float_types)
        )
        if numeric_fields != contracted_fields or set(wire.integer_types) & set(
            wire.float_types
        ):
            raise ValueError(
                f"DDGI validation wire contract does not cover the canonical record in "
                f"{evidence_path}"
            )
        for field, type_name in wire.integer_types.items():
            require_rust_unsigned(int(record[field]), type_name, field, evidence_path)
        for field, type_name in wire.optional_integer_types.items():
            value = record[field]
            if value is not None:
                require_rust_unsigned(int(value), type_name, field, evidence_path)
        for field in wire.float_types:
            require_nonnegative_f32(float(record[field]), field, evidence_path)

        serial = int(record["field_serial"])
        radiance_revision = int(record["radiance_revision"])
        spacing_voxels = int(record["spacing_voxels"])
        state = str(record["state"])
        epoch = int(record["update_epoch"])
        source_field_serial = record["source_field_serial"]
        if serial == 0 or radiance_revision == 0 or spacing_voxels == 0:
            raise ValueError(
                f"typed field identity in {evidence_path} has a zero serial, radiance "
                f"revision, or spacing: serial={serial} radiance={radiance_revision} "
                f"spacing={spacing_voxels}"
            )
        if state == "Converged" and epoch == 0:
            raise ValueError(
                f"typed field identity in {evidence_path} cannot be Converged at epoch zero"
            )
        consecutive = int(record["consecutive_below_threshold"])
        if source_field_serial is None:
            if epoch != 0 or state != "Converging" or consecutive != 0:
                raise ValueError(
                    f"source-free field in {evidence_path} must be Converging epoch zero "
                    f"with streak zero"
                )
        if record["nonfinite_count"] != 0 or record["negative_rgb_texel_count"] != 0:
            raise ValueError(f"validated atlas record in {evidence_path} has invalid texels")
        ValidatedAtlasCoverage.from_record(record).require_complete(
            provenance.probe_population,
            provenance.atlas_layout,
            evidence_path,
        )
        if record["required_consecutive_epochs"] != policy.consecutive_epochs:
            raise ValueError(f"validation record in {evidence_path} has consecutive policy drift")

    field_serials = [int(record["field_serial"]) for record in records]
    if len(set(field_serials)) != len(field_serials) or any(
        left >= right for left, right in zip(field_serials, field_serials[1:])
    ):
        raise ValueError(
            f"global validation order in {evidence_path} has duplicate or unordered "
            f"field serials: {field_serials}"
        )

    prior_records_by_serial: dict[int, dict[str, object]] = {}
    for record in records:
        serial = int(record["field_serial"])
        source_field_serial = record["source_field_serial"]
        if source_field_serial is not None:
            source_serial = int(source_field_serial)
            source = prior_records_by_serial.get(source_serial)
            if source_serial == 0 or source is None:
                raise ValueError(
                    f"global lineage source field serial {source_serial} in {evidence_path} does not "
                    "reference an earlier process-bound validation"
                )
            if int(source["spacing_voxels"]) != int(record["spacing_voxels"]):
                raise ValueError(
                    f"global lineage source field serial {source_serial} in {evidence_path} has spacing "
                    f"{source['spacing_voxels']}, destination {serial} has spacing "
                    f"{record['spacing_voxels']}"
                )
            same_transport_revision = (
                source["geometry_revision"] == record["geometry_revision"]
                and source["radiance_revision"] == record["radiance_revision"]
            )
            expected_epoch = (
                min(int(source["update_epoch"]) + 1, (1 << 32) - 1)
                if same_transport_revision
                else 0
            )
            if int(record["update_epoch"]) != expected_epoch:
                raise ValueError(
                    f"global lineage source field serial {source_serial} in {evidence_path} requires "
                    f"destination {serial} epoch {expected_epoch}, found "
                    f"{record['update_epoch']}"
                )
        prior_records_by_serial[serial] = record

    generation_identity: tuple[int, int, int] | None = None
    generations: list[list[dict[str, object]]] = []
    previous_record: dict[str, object] | None = None
    previous_consecutive = 0
    active_converged = False
    for record in records:
        identity = (
            int(record["geometry_revision"]),
            int(record["radiance_revision"]),
            int(record["spacing_voxels"]),
        )
        epoch = int(record["update_epoch"])
        if epoch == 0:
            generation_identity = identity
            generations.append([])
            previous_consecutive = 0
            active_converged = False
        else:
            if previous_record is None:
                raise ValueError(
                    f"global lineage in {evidence_path} starts at nonzero epoch {epoch}"
                )
            expected_epoch = int(previous_record["update_epoch"]) + 1
            expected_source = int(previous_record["field_serial"])
            if (
                generation_identity != identity
                or epoch != expected_epoch
                or record["source_field_serial"] != expected_source
            ):
                raise ValueError(
                    f"global lineage in {evidence_path} has identity={identity} e{epoch} "
                    f"source={record['source_field_serial']}, expected "
                    f"identity={generation_identity} e{expected_epoch} "
                    f"source={expected_source}"
                )
            if active_converged:
                raise ValueError(
                    f"global lineage in {evidence_path} continued a converged generation"
                )
        possible_below = possible_below_threshold(
            record, policy, wire.decimal_places
        )
        consecutive = int(record["consecutive_below_threshold"])
        if epoch == 0:
            allowed = (
                {0}
                if record["source_field_serial"] is None
                else {1 if below else 0 for below in possible_below}
            )
            if consecutive not in allowed:
                raise ValueError(
                    f"global consecutive sequence in {evidence_path} starts with "
                    f"{consecutive}, allowed {sorted(allowed)} for identity {identity}"
                )
        else:
            expected_consecutive = {
                previous_consecutive + 1 if below else 0 for below in possible_below
            }
            if consecutive not in expected_consecutive:
                raise ValueError(
                    f"global consecutive sequence in {evidence_path} has {consecutive}, "
                    f"expected one of {sorted(expected_consecutive)} for identity "
                    f"{identity} at e{epoch}"
                )
        completed_epoch_count = min(epoch + 1, (1 << 32) - 1)
        should_converge = (
            completed_epoch_count >= policy.minimum_epoch_count
            and consecutive >= policy.consecutive_epochs
        ) or completed_epoch_count >= policy.maximum_update_epoch + 1
        is_converged = record["state"] == "Converged"
        if is_converged != should_converge:
            raise ValueError(
                f"global convergence state in {evidence_path} is {record['state']} at "
                f"e{epoch}, expected {'Converged' if should_converge else 'Converging'}"
            )
        previous_consecutive = consecutive
        active_converged = is_converged
        generations[-1].append(record)
        previous_record = record
    return generations


def expected_terminal_reason(record: dict[str, object], policy: Policy) -> str | None:
    completed_epoch_count = min(int(record["update_epoch"]) + 1, (1 << 32) - 1)
    threshold_converged = (
        completed_epoch_count >= policy.minimum_epoch_count
        and int(record["consecutive_below_threshold"])
        >= policy.consecutive_epochs
    )
    if threshold_converged:
        return "Threshold"
    if completed_epoch_count >= policy.maximum_update_epoch + 1:
        return "SampleBudget"
    return None


def parse_curve(
    console_path: Path, contract_path: Path = CONTRACT_PATH
) -> tuple[
    list[list[dict[str, object]]],
    TerminalIdentity,
    ConvergenceProvenance,
    ValidationWireContract,
]:
    records: list[dict[str, object]] = []
    terminals: list[TerminalIdentity] = []
    text = console_path.read_text()
    policy_matches = list(POLICY_PATTERN.finditer(text))
    if len(policy_matches) != 1:
        raise ValueError(
            f"expected exactly one authoritative runtime convergence policy in "
            f"{console_path}, found {len(policy_matches)}"
        )
    policy_values = policy_matches[0].groupdict()
    policy_line = policy_matches[0].group(0)
    maximum_update_epochs = int(policy_values["maximum"])
    if maximum_update_epochs == 0:
        raise ValueError("runtime convergence maximum_update_epochs must be positive")
    policy = Policy(
        float(policy_values["absolute"]),
        float(policy_values["relative"]),
        float(policy_values["relative_floor"]),
        int(policy_values["consecutive"]),
        int(policy_values["minimum"]),
        maximum_update_epochs - 1,
    )
    terrain_revision = int(policy_values["terrain"])
    policy_spacing = int(policy_values["policy_spacing"])
    probe_count = int(policy_values["probes"])
    if not 0 <= terrain_revision <= (1 << 32) - 1:
        raise ValueError("runtime initialization terrain revision exceeds u32")
    if not 0 < policy_spacing <= (1 << 32) - 1:
        raise ValueError("runtime initialization spacing must be a positive u32")
    if not 0 < probe_count <= (1 << 32) - 1:
        raise ValueError("runtime initialization probe count must be a positive u32")
    expected_probe_count = expected_initialization_probe_count(
        contract_path, policy_spacing
    )
    if probe_count != expected_probe_count:
        raise ValueError(
            f"runtime initialization probe count {probe_count} differs from "
            f"acceptance grid {expected_probe_count} at spacing {policy_spacing}"
        )
    initialization = InitializationEvent(
        log_time=policy_values["log_time"],
        terrain_revision=terrain_revision,
        spacing_voxels=policy_spacing,
        probe_count=probe_count,
        stage=policy_values["stage"],
        policy=policy,
    )
    population_matches = list(RELOCATION_POPULATION_PATTERN.finditer(text))
    if len(population_matches) != 1:
        raise ValueError(
            f"expected exactly one authoritative DDGI relocation population in "
            f"{console_path}, found {len(population_matches)}"
        )
    population = ProbePopulation.from_match(population_matches[0], console_path)
    if population.total_count != initialization.probe_count:
        raise ValueError(
            f"DDGI relocation population total {population.total_count} in {console_path} "
            f"differs from initialization probe count {initialization.probe_count}"
        )
    provenance = ConvergenceProvenance(
        initialization=initialization,
        probe_population=population,
        atlas_layout=AtlasTexelLayout.load(contract_path),
    )
    require_policy_wire_legality(policy, maximum_update_epochs, console_path)
    require_policy_matches_contract(policy, load_acceptance_contract(contract_path))
    if policy_line != canonical_policy_line(policy_matches[0], contract_path):
        raise ValueError(
            f"noncanonical authoritative runtime convergence policy in {console_path}"
        )
    wire = load_validation_wire_contract(contract_path)
    awaiting_terminal: dict[str, object] | None = None
    for line in text.splitlines():
        evidence_marker_count = line.count(EVIDENCE_MARKER)
        if evidence_marker_count not in (0, 1):
            raise ValueError(
                f"expected exactly one DDGI convergence evidence marker on each "
                f"physical line in {console_path}: {line}"
            )
        if evidence_marker_count == 1 and not (
            VALIDATION_MARKER in line or TERMINAL_MARKER in line
        ):
            raise ValueError(
                f"malformed DDGI convergence evidence in {console_path}: {line}"
            )
        if VALIDATION_MARKER in line:
            if awaiting_terminal is not None:
                raise ValueError(
                    f"Converged validation in {console_path} is missing its immediate "
                    f"terminal marker event"
                )
            match = VALIDATION_PATTERN.fullmatch(line)
            if match is None:
                raise ValueError(
                    f"malformed full-atlas validation line in {console_path}: {line}"
                )
            values = match.groupdict()
            record: dict[str, object] = {
                "log_time": values["log_time"],
                "field_serial": int(values["field_serial"]),
                "source_field_serial": (
                    None if values["source"] == "none" else int(values["source"])
                ),
                "geometry_revision": int(values["geometry"]),
                "radiance_revision": int(values["radiance"]),
                "spacing_voxels": int(values["spacing"]),
                "state": values["state"],
                "update_epoch": int(values["epoch"]),
                "max_absolute_rgb_delta": float(values["absolute"]),
                "max_relative_rgb_delta": float(values["relative"]),
                "nonfinite_count": int(values["nonfinite"]),
                "negative_rgb_texel_count": int(values["negative"]),
                "valid_texel_count": int(values["valid"]),
                "scanned_stored_texel_count": int(values["scanned"]),
                "absolute_threshold": float(values["absolute_threshold"]),
                "relative_threshold": float(values["relative_threshold"]),
                "consecutive_below_threshold": int(values["consecutive"]),
                "required_consecutive_epochs": int(values["required"]),
            }
            marker_payload = line[line.index(EVIDENCE_MARKER) :]
            if marker_payload != canonical_validation_line(record, policy, wire):
                raise ValueError(
                    f"noncanonical full-atlas validation line in {console_path}: {line}"
                )
            records.append(record)
            if record["state"] == "Converged":
                awaiting_terminal = record
        if TERMINAL_MARKER in line:
            if awaiting_terminal is None:
                raise ValueError(
                    f"orphan or premature terminal marker event in {console_path}: {line}"
                )
            match = TERMINAL_PATTERN.fullmatch(line)
            if match is None:
                raise ValueError(f"malformed convergence line in {console_path}: {line}")
            values = match.groupdict()
            terminal = TerminalIdentity(
                log_time=values["log_time"],
                field_serial=int(values["field_serial"]),
                geometry_revision=int(values["geometry"]),
                radiance_revision=int(values["radiance"]),
                spacing_voxels=int(values["spacing"]),
                update_epoch=int(values["epoch"]),
                reason=values["reason"],
            )
            marker_payload = line[line.index(EVIDENCE_MARKER) :]
            if marker_payload != canonical_terminal_line(terminal):
                raise ValueError(
                    f"noncanonical terminal line in {console_path}: {line}"
                )
            for field in (
                "field_serial",
                "geometry_revision",
                "radiance_revision",
                "spacing_voxels",
                "update_epoch",
            ):
                if getattr(terminal, field) != awaiting_terminal[field]:
                    raise ValueError(
                        f"terminal {field} does not match its preceding Converged validation"
                    )
            expected_reason = expected_terminal_reason(awaiting_terminal, policy)
            if terminal.reason != expected_reason:
                raise ValueError(
                    f"terminal reason {terminal.reason} does not match its preceding "
                    f"Converged validation reason {expected_reason}"
                )
            terminals.append(terminal)
            awaiting_terminal = None
    if awaiting_terminal is not None:
        raise ValueError(
            f"Converged validation in {console_path} is missing its terminal marker event"
        )
    if not records:
        raise ValueError(f"no full-atlas validation records in {console_path}")
    generations = require_global_validation_legality(
        records, console_path, wire, policy, provenance
    )
    if len(terminals) != 1:
        raise ValueError(
            f"expected exactly one terminal convergence record in {console_path}, "
            f"found {len(terminals)}"
        )
    terminal = terminals[0]
    final = records[-1]
    for field in (
        "field_serial",
        "geometry_revision",
        "radiance_revision",
        "spacing_voxels",
        "update_epoch",
    ):
        if getattr(terminal, field) != final[field]:
            raise ValueError(f"terminal {field} does not match final validation record")
    return generations, terminal, provenance, wire


def validate_curve(
    case_name: str,
    spacing: int,
    generations: list[list[dict[str, object]]],
    terminal: TerminalIdentity,
    analysis: dict[str, object],
    provenance: ConvergenceProvenance,
    wire: ValidationWireContract,
) -> dict[str, object]:
    initialization = provenance.initialization
    policy = initialization.policy
    capture = analysis.get("capture")
    if not isinstance(capture, dict):
        raise ValueError(f"{case_name} spacing {spacing}: analysis has no capture object")
    if analysis.get("validation_failures") != []:
        raise ValueError(
            f"{case_name} spacing {spacing}: analyzer failures: "
            f"{analysis.get('validation_failures')}"
        )
    if capture.get("lifecycle_state") != "converged":
        raise ValueError(f"{case_name} spacing {spacing}: capture is not converged")
    integer_limits = {
        "field_serial": (1, (1 << 64) - 1),
        "geometry_revision": (0, (1 << 32) - 1),
        "radiance_revision": (0, (1 << 32) - 1),
        "spacing_voxels": (1, (1 << 32) - 1),
        "update_epoch": (0, (1 << 32) - 1),
    }
    for field, (minimum, maximum) in integer_limits.items():
        value = capture.get(field)
        if type(value) is not int or not minimum <= value <= maximum:
            raise ValueError(
                f"{case_name} spacing {spacing}: capture {field} is not a valid "
                f"{'u64' if maximum > (1 << 32) - 1 else 'u32'}"
            )
    for field in ("max_abs_delta", "max_rel_delta"):
        value = capture.get(field)
        if type(value) not in (int, float) or not math.isfinite(float(value)):
            raise ValueError(
                f"{case_name} spacing {spacing}: capture {field} is not finite numeric evidence"
            )
    if initialization.spacing_voxels != spacing:
        raise ValueError(
            f"{case_name} spacing {spacing}: initialization spacing "
            f"{initialization.spacing_voxels} differs from the requested curve"
        )
    if capture["spacing_voxels"] != spacing:
        raise ValueError(f"{case_name} spacing {spacing}: capture spacing mismatch")
    geometry_revision = capture["geometry_revision"]
    radiance_revision = capture["radiance_revision"]
    field_serial = capture["field_serial"]
    if geometry_revision != initialization.terrain_revision:
        raise ValueError(
            f"{case_name} spacing {spacing}: capture geometry revision "
            f"{geometry_revision} differs from initialization terrain revision "
            f"{initialization.terrain_revision}"
        )
    matching_generations = [
        generation
        for generation in generations
        if any(record["field_serial"] == field_serial for record in generation)
    ]
    if len(matching_generations) != 1:
        raise ValueError(
            f"{case_name} spacing {spacing}: expected one lineage generation for captured "
            f"field serial {field_serial}, found {len(matching_generations)}"
        )
    records = matching_generations[0]
    if any(
        record["geometry_revision"] != geometry_revision
        or record["radiance_revision"] != radiance_revision
        or record["spacing_voxels"] != spacing
        for record in records
    ):
        raise ValueError(
            f"{case_name} spacing {spacing}: captured lineage tuple mismatch"
        )

    first_threshold_epoch = next(
        (
            int(record["update_epoch"])
            for record in records
            if int(record["update_epoch"]) + 1 >= policy.minimum_epoch_count
            and int(record["consecutive_below_threshold"])
            >= policy.consecutive_epochs
        ),
        None,
    )

    final = records[-1]
    if final["field_serial"] != field_serial:
        raise ValueError(f"{case_name} spacing {spacing}: capture field serial mismatch")
    final_epoch = int(final["update_epoch"])
    for field in (
        "field_serial",
        "geometry_revision",
        "radiance_revision",
        "spacing_voxels",
        "update_epoch",
    ):
        terminal_value = getattr(terminal, field)
        if terminal_value != final[field]:
            raise ValueError(
                f"{case_name} spacing {spacing}: terminal {field} differs from "
                "the captured curve final record"
            )
        if terminal_value != capture.get(field):
            raise ValueError(
                f"{case_name} spacing {spacing}: terminal {field} differs from capture"
            )
    expected_reason = (
        "Threshold" if first_threshold_epoch == final_epoch else "SampleBudget"
    )
    if first_threshold_epoch is not None and first_threshold_epoch != final_epoch:
        raise ValueError(f"{case_name} spacing {spacing}: curve continued after threshold sleep")
    if first_threshold_epoch is None and final_epoch != policy.maximum_update_epoch:
        raise ValueError(f"{case_name} spacing {spacing}: sample budget ended at e{final_epoch}")
    if terminal.reason != expected_reason:
        raise ValueError(
            f"{case_name} spacing {spacing}: terminal reason {terminal.reason}, "
            f"expected {expected_reason}"
        )
    if capture.get("update_epoch") != final_epoch:
        raise ValueError(f"{case_name} spacing {spacing}: capture epoch mismatch")
    if validation_float_token(
        float(capture["max_abs_delta"]), wire.decimal_places
    ) != validation_float_token(
        float(final["max_absolute_rgb_delta"]), wire.decimal_places
    ):
        raise ValueError(f"{case_name} spacing {spacing}: capture absolute delta mismatch")
    if validation_float_token(
        float(capture["max_rel_delta"]), wire.decimal_places
    ) != validation_float_token(
        float(final["max_relative_rgb_delta"]), wire.decimal_places
    ):
        raise ValueError(f"{case_name} spacing {spacing}: capture relative delta mismatch")

    return {
        "case": case_name,
        "spacing_voxels": spacing,
        "qualified": True,
        "final_update_epoch": final_epoch,
        "terminal_reason": terminal.reason,
        "final_max_absolute_rgb_delta": float(final["max_absolute_rgb_delta"]),
        "final_max_relative_rgb_delta": float(final["max_relative_rgb_delta"]),
        "initialization": {
            "log_time": initialization.log_time,
            "terrain_revision": initialization.terrain_revision,
            "spacing_voxels": initialization.spacing_voxels,
            "probe_count": initialization.probe_count,
            "stage": initialization.stage,
        },
        "epochs": records,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cases", nargs="+", default=list(DEFAULT_CASES))
    parser.add_argument("--spacings", nargs="+", type=int, default=list(DEFAULT_SPACINGS))
    parser.add_argument("--contract", type=Path, default=CONTRACT_PATH)
    args = parser.parse_args()

    try:
        curves = []
        policy: Policy | None = None
        for spacing in args.spacings:
            for case_name in args.cases:
                stem = f"{case_name}-spacing{spacing}-converged-forward"
                console_path = args.run_dir / f"{stem}.console.log"
                run_log_path = args.run_dir / f"{stem}.run.log"
                analysis_path = args.run_dir / f"{stem}.analysis.json"
                console_evidence = parse_curve(
                    console_path, args.contract
                )
                run_log_evidence = parse_curve(run_log_path, args.contract)
                if console_evidence != run_log_evidence:
                    raise ValueError(
                        f"{case_name} spacing {spacing}: console and preserved run-log "
                        "convergence evidence differ"
                    )
                generations, terminal, provenance, wire = console_evidence
                initialization = provenance.initialization
                runtime_policy = initialization.policy
                if policy is None:
                    policy = runtime_policy
                elif runtime_policy != policy:
                    raise ValueError(
                        f"{case_name} spacing {spacing}: runtime convergence policy drift"
                    )
                curve = validate_curve(
                    case_name,
                    spacing,
                    generations,
                    terminal,
                    json.loads(analysis_path.read_text()),
                    provenance,
                    wire,
                )
                curve["capture_analysis"] = analysis_path.name
                curve["console_log"] = console_path.name
                curve["preserved_run_log"] = run_log_path.name
                curves.append(curve)
        if policy is None:
            raise ValueError("convergence matrix is empty")
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"DDGI convergence provenance validation failed: {error}", file=sys.stderr)
        return 1

    result = {
        "schema_version": 2,
        "qualified": True,
        "matrix": {
            "cases": args.cases,
            "spacings_voxels": args.spacings,
            "curve_count": len(curves),
        },
        "policy": {
            "max_absolute_rgb_delta": policy.absolute_threshold,
            "max_relative_rgb_delta": policy.relative_threshold,
            "relative_floor": policy.relative_floor,
            "consecutive_epochs": policy.consecutive_epochs,
            "minimum_epoch_count": policy.minimum_epoch_count,
            "maximum_update_epoch": policy.maximum_update_epoch,
        },
        "curves": curves,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(
        f"[DDGI_CONVERGENCE] PASS curves={len(curves)} output={args.output} "
        f"maximum_update_epoch={policy.maximum_update_epoch}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
