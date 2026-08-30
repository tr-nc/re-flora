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
VALIDATION_PATTERN = re.compile(
    re.escape(VALIDATION_MARKER)
    + r" field_serial=(?P<field_serial>\d+) geometry_revision=(?P<geometry>\d+) "
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
    re.escape(TERMINAL_MARKER)
    + r" field_serial=(?P<field_serial>\d+) geometry_revision=(?P<geometry>\d+) "
    r"radiance_revision=(?P<radiance>\d+) "
    r"spacing_voxels=(?P<spacing>\d+) "
    r"update_epoch=(?P<epoch>\d+) reason=(?P<reason>Threshold|SampleBudget)$"
)
POLICY_PATTERN = re.compile(
    r"initialization requested .*?"
    r"convergence_max_absolute_rgb_delta=(?P<absolute>[0-9.eE+-]+) "
    r"convergence_max_relative_rgb_delta=(?P<relative>[0-9.eE+-]+) "
    r"convergence_relative_floor=(?P<relative_floor>[0-9.eE+-]+) "
    r"convergence_consecutive_epochs=(?P<consecutive>\d+) "
    r"convergence_minimum_update_epochs=(?P<minimum>\d+) "
    r"convergence_maximum_update_epochs=(?P<maximum>\d+)"
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
class TerminalIdentity:
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
    float_types: dict[str, str]


def close(left: float, right: float) -> bool:
    return math.isclose(left, right, rel_tol=0.0, abs_tol=5.0e-8)


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


def load_validation_wire_contract(path: Path) -> ValidationWireContract:
    contract = tomllib.loads(path.read_text())
    wire = contract.get("validation_wire")
    if not isinstance(wire, dict):
        raise ValueError("missing DDGI convergence validation wire contract")
    integer_types = wire.get("integer_types")
    float_types = wire.get("float_types")
    decimal_places = wire.get("decimal_places")
    if not isinstance(integer_types, dict) or not isinstance(float_types, dict):
        raise ValueError("invalid DDGI convergence validation wire contract")
    if (
        not isinstance(decimal_places, int)
        or isinstance(decimal_places, bool)
        or decimal_places <= 0
    ):
        raise ValueError("invalid DDGI convergence validation decimal precision")
    if any(type_name not in ("u32", "u64") for type_name in integer_types.values()):
        raise ValueError("unsupported DDGI convergence integer wire type")
    if any(type_name != "f32" for type_name in float_types.values()):
        raise ValueError("unsupported DDGI convergence float wire type")
    return ValidationWireContract(
        decimal_places, dict(integer_types), dict(float_types)
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
) -> None:
    for record in records:
        numeric_fields = set(record) - {"state"}
        contracted_fields = set(wire.integer_types) | set(wire.float_types)
        if numeric_fields != contracted_fields or set(wire.integer_types) & set(
            wire.float_types
        ):
            raise ValueError(
                f"DDGI validation wire contract does not cover the canonical record in "
                f"{evidence_path}"
            )
        for field, type_name in wire.integer_types.items():
            require_rust_unsigned(int(record[field]), type_name, field, evidence_path)
        for field in wire.float_types:
            require_nonnegative_f32(float(record[field]), field, evidence_path)

        serial = int(record["field_serial"])
        radiance_revision = int(record["radiance_revision"])
        spacing_voxels = int(record["spacing_voxels"])
        state = str(record["state"])
        epoch = int(record["update_epoch"])
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
        if record["nonfinite_count"] != 0 or record["negative_rgb_texel_count"] != 0:
            raise ValueError(f"validated atlas record in {evidence_path} has invalid texels")
        valid = int(record["valid_texel_count"])
        scanned = int(record["scanned_stored_texel_count"])
        if valid == 0 or scanned == 0 or scanned * 64 != valid * 100:
            raise ValueError(
                f"validated atlas record in {evidence_path} has incomplete coverage"
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

    completed_identities: set[tuple[int, int, int]] = set()
    active_identity: tuple[int, int, int] | None = None
    next_epoch = 0
    previous_consecutive = 0
    active_converged = False
    for record_index, record in enumerate(records):
        identity = (
            int(record["geometry_revision"]),
            int(record["radiance_revision"]),
            int(record["spacing_voxels"]),
        )
        if identity != active_identity:
            if identity in completed_identities:
                raise ValueError(
                    f"global validation order in {evidence_path} returned to completed "
                    f"identity {identity}"
                )
            if active_identity is not None:
                completed_identities.add(active_identity)
            active_identity = identity
            next_epoch = 0
            previous_consecutive = 0
            active_converged = False
        elif active_converged:
            raise ValueError(
                f"global validation order in {evidence_path} continued converged identity "
                f"{identity}"
            )
        epoch = int(record["update_epoch"])
        if epoch != next_epoch:
            raise ValueError(
                f"global validation order in {evidence_path} has epoch {epoch}, "
                f"expected {next_epoch} for identity {identity}"
            )
        possible_below = possible_below_threshold(
            record, policy, wire.decimal_places
        )
        consecutive = int(record["consecutive_below_threshold"])
        if epoch == 0:
            allowed = (
                {0}
                if record_index == 0
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
        next_epoch += 1


def parse_curve(
    console_path: Path, contract_path: Path = CONTRACT_PATH
) -> tuple[list[dict[str, object]], TerminalIdentity, Policy]:
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
    require_policy_wire_legality(policy, maximum_update_epochs, console_path)
    require_policy_matches_contract(policy, load_acceptance_contract(contract_path))
    wire = load_validation_wire_contract(contract_path)
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
            match = VALIDATION_PATTERN.search(line)
            if match is None:
                raise ValueError(
                    f"malformed full-atlas validation line in {console_path}: {line}"
                )
            values = match.groupdict()
            expected_absolute_threshold = validation_float_token(
                policy.absolute_threshold, wire.decimal_places
            )
            expected_relative_threshold = validation_float_token(
                policy.relative_threshold, wire.decimal_places
            )
            if values["absolute_threshold"] != expected_absolute_threshold:
                raise ValueError(
                    f"validation record in {console_path} has absolute Rust f32 policy drift"
                )
            if values["relative_threshold"] != expected_relative_threshold:
                raise ValueError(
                    f"validation record in {console_path} has relative Rust f32 policy drift"
                )
            records.append(
                {
                    "field_serial": int(values["field_serial"]),
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
            )
        if TERMINAL_MARKER in line:
            match = TERMINAL_PATTERN.search(line)
            if match is None:
                raise ValueError(f"malformed convergence line in {console_path}: {line}")
            values = match.groupdict()
            terminals.append(
                TerminalIdentity(
                    field_serial=int(values["field_serial"]),
                    geometry_revision=int(values["geometry"]),
                    radiance_revision=int(values["radiance"]),
                    spacing_voxels=int(values["spacing"]),
                    update_epoch=int(values["epoch"]),
                    reason=values["reason"],
                )
            )
    if not records:
        raise ValueError(f"no full-atlas validation records in {console_path}")
    require_global_validation_legality(records, console_path, wire, policy)
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
    return records, terminal, policy


def validate_curve(
    case_name: str,
    spacing: int,
    records: list[dict[str, object]],
    terminal: TerminalIdentity,
    analysis: dict[str, object],
    policy: Policy,
) -> dict[str, object]:
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
    if capture.get("spacing_voxels") != spacing:
        raise ValueError(f"{case_name} spacing {spacing}: capture spacing mismatch")
    geometry_revision = capture.get("geometry_revision")
    radiance_revision = capture.get("radiance_revision")
    field_serial = capture.get("field_serial")
    if not isinstance(field_serial, int):
        raise ValueError(f"{case_name} spacing {spacing}: capture has no field serial")
    records = [
        record
        for record in records
        if record["geometry_revision"] == geometry_revision
        and record["radiance_revision"] == radiance_revision
        and record["spacing_voxels"] == spacing
    ]
    if not records:
        raise ValueError(
            f"{case_name} spacing {spacing}: no curve for captured geometry/radiance revision"
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
    if not close(float(capture["max_abs_delta"]), float(final["max_absolute_rgb_delta"])):
        raise ValueError(f"{case_name} spacing {spacing}: capture absolute delta mismatch")
    if not close(float(capture["max_rel_delta"]), float(final["max_relative_rgb_delta"])):
        raise ValueError(f"{case_name} spacing {spacing}: capture relative delta mismatch")

    return {
        "case": case_name,
        "spacing_voxels": spacing,
        "qualified": True,
        "final_update_epoch": final_epoch,
        "terminal_reason": terminal.reason,
        "final_max_absolute_rgb_delta": float(final["max_absolute_rgb_delta"]),
        "final_max_relative_rgb_delta": float(final["max_relative_rgb_delta"]),
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
                records, terminal, runtime_policy = console_evidence
                if policy is None:
                    policy = runtime_policy
                elif runtime_policy != policy:
                    raise ValueError(
                        f"{case_name} spacing {spacing}: runtime convergence policy drift"
                    )
                curve = validate_curve(
                    case_name,
                    spacing,
                    records,
                    terminal,
                    json.loads(analysis_path.read_text()),
                    runtime_policy,
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
