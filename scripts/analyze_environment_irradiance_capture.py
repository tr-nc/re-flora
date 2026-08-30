#!/usr/bin/env python3
"""Inspect deterministic pre-albedo environment-irradiance captures."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import struct
from dataclasses import dataclass
from pathlib import Path


MAGIC = b"RFIRR001"
HEADER_PREFIX = struct.Struct("<8sI")
HEADER_V1 = struct.Struct("<8s6I")
HEADER_V2 = struct.Struct("<8s7I")
HEADER_V3 = struct.Struct("<8s10I2Q4IQI2f2I")
HEADER_V4 = struct.Struct("<8s10I3Q4IQ3I2f2I")
HEADER_V5 = HEADER_V4
HEADER_V6 = HEADER_V4
HEADER_V7 = HEADER_V4
HEADER_V8 = HEADER_V4
PIXEL = struct.Struct("<4f")
UNKNOWN_U32 = 0xFFFFFFFF
UNKNOWN_U64 = 0xFFFFFFFFFFFFFFFF
UNKNOWN_DELTA = -1.0

DEBUG_VIEW_LABELS = {
    0: "final",
    1: "moment-visibility",
    2: "exact-visibility",
    3: "visibility-error",
    4: "exact-irradiance",
    5: "irradiance-error",
    6: "weight-sum",
    7: "dominant-probe",
    8: "probe-state",
    9: "relocation",
    10: "irradiance-atlas",
    11: "visibility-atlas",
    12: "unoccluded-irradiance",
    13: "equal-weight-irradiance",
    14: "raw-cage-irradiance",
}
TRANSPORT_STAGE_LABELS = {
    1: "seed-sky",
    2: "single-bounce",
    3: "feedback",
    4: "converged",
    5: "non-converged",
}
LIFECYCLE_STATE_LABELS = {
    1: "converging",
    2: "converged",
}
PUBLICATION_STATE_LABELS = {
    0: "unpublished",
    1: "published",
}
BATCH_ORDER_LABELS = {
    0: "forward",
    1: "reverse",
}
ROI_CHANNEL_INDICES = {
    "red": 0,
    "green": 1,
    "blue": 2,
}
WORLD_ROI_BOUNDARY_EPSILON = 1.0e-6
TERRAIN_VOXELS_PER_WORLD_UNIT = 256.0
VOXEL_FACE_BOUNDARY_EPSILON = 1.0e-3
VOXEL_FACE_MIN_MIXED_CLASS_PIXELS = 2
RECEIVER_VOXEL_INTERIOR_NUDGE = 1.0e-3
DIRECT_LIGHT_RECEIVER_VOXEL_MIN_PIXELS = 4
CAPTURED_RECEIVER_CENTER_EPSILON = 1.0e-3


@dataclass(frozen=True)
class Capture:
    path: Path
    version: int
    width: int
    height: int
    backend: int
    spacing_voxels: int
    debug_view: int
    payload: bytes
    world_payload: bytes = b""
    direct_light_payload: bytes = b""
    terrain_shadow_receiver_payload: bytes = b""
    direct_sun_shadow_payload: bytes = b""
    plane_count: int = 1
    geometry_revision: int | None = None
    radiance_revision: int | None = None
    radiance_model_identity: int | None = None
    build_token_serial: int | None = None
    field_serial: int | None = None
    transport_stage: int | None = None
    transport_iteration: int | None = None
    source_stage: int | None = None
    source_iteration: int | None = None
    source_identity: int | None = None
    source_field_serial: int | None = None
    source_radiance_revision: int | None = None
    lifecycle_state: int | None = None
    update_epoch: int | None = None
    source_state: int | None = None
    source_update_epoch: int | None = None
    publication_state: int | None = None
    batch_order: int | None = None
    max_abs_delta: float | None = None
    max_rel_delta: float | None = None
    nonfinite_count: int | None = None
    valid_count: int | None = None

    @property
    def sample_count(self) -> int:
        return self.width * self.height

    @property
    def token_serial(self) -> int | None:
        """Backward-compatible name for the volume build-attempt serial."""
        return self.build_token_serial


def load_capture(path: Path) -> Capture:
    data = path.read_bytes()
    if len(data) < HEADER_PREFIX.size:
        raise ValueError(f"{path}: truncated header")
    magic, version = HEADER_PREFIX.unpack_from(data)
    if magic != MAGIC:
        raise ValueError(f"{path}: invalid magic {magic!r}")
    if version == 1:
        if len(data) < HEADER_V1.size:
            raise ValueError(f"{path}: truncated v1 header")
        _, _, width, height, channels, backend, spacing = HEADER_V1.unpack_from(data)
        debug_view = 0
        header_size = HEADER_V1.size
        plane_count = 1
        metadata: dict[str, object] = {}
    elif version == 2:
        if len(data) < HEADER_V2.size:
            raise ValueError(f"{path}: truncated v2 header")
        _, _, width, height, channels, backend, spacing, debug_view = HEADER_V2.unpack_from(data)
        header_size = HEADER_V2.size
        plane_count = 1
        metadata = {}
    elif version == 3:
        if len(data) < HEADER_V3.size:
            raise ValueError(f"{path}: truncated v3 header")
        (
            _,
            _,
            width,
            height,
            channels,
            backend,
            spacing,
            debug_view,
            plane_count,
            geometry_revision,
            radiance_revision,
            radiance_model_identity,
            token_serial,
            transport_stage,
            transport_iteration,
            source_stage,
            source_iteration,
            source_identity,
            publication_state,
            max_abs_delta,
            max_rel_delta,
            nonfinite_count,
            valid_count,
        ) = HEADER_V3.unpack_from(data)
        header_size = HEADER_V3.size
        metadata = {
            "geometry_revision": None if geometry_revision == UNKNOWN_U32 else geometry_revision,
            "radiance_revision": None if radiance_revision == UNKNOWN_U32 else radiance_revision,
            "radiance_model_identity": None if radiance_model_identity == UNKNOWN_U64 else radiance_model_identity,
            "build_token_serial": None if token_serial == UNKNOWN_U64 else token_serial,
            "transport_stage": None if transport_stage == UNKNOWN_U32 else transport_stage,
            "transport_iteration": None if transport_iteration == UNKNOWN_U32 else transport_iteration,
            "source_stage": None if source_stage == UNKNOWN_U32 else source_stage,
            "source_iteration": None if source_iteration == UNKNOWN_U32 else source_iteration,
            "source_identity": None if source_identity == UNKNOWN_U64 else source_identity,
            "publication_state": None if publication_state == UNKNOWN_U32 else publication_state,
            "max_abs_delta": None if max_abs_delta == UNKNOWN_DELTA else max_abs_delta,
            "max_rel_delta": None if max_rel_delta == UNKNOWN_DELTA else max_rel_delta,
            "nonfinite_count": None if nonfinite_count == UNKNOWN_U32 else nonfinite_count,
            "valid_count": None if valid_count == UNKNOWN_U32 else valid_count,
        }
    elif version in (4, 5, 6, 7, 8):
        header = {
            4: HEADER_V4,
            5: HEADER_V5,
            6: HEADER_V6,
            7: HEADER_V7,
            8: HEADER_V8,
        }[version]
        if len(data) < header.size:
            raise ValueError(f"{path}: truncated v{version} header")
        (
            _,
            _,
            width,
            height,
            channels,
            backend,
            spacing,
            debug_view,
            plane_count,
            geometry_revision,
            radiance_revision,
            radiance_model_identity,
            build_token_serial,
            field_serial,
            state_or_stage,
            epoch_or_iteration,
            source_state_or_stage,
            source_epoch_or_iteration,
            source_field_serial,
            source_radiance_revision,
            publication_state,
            batch_order,
            max_abs_delta,
            max_rel_delta,
            nonfinite_count,
            valid_count,
        ) = header.unpack_from(data)
        header_size = header.size
        lifecycle_metadata = (
            {
                "lifecycle_state": None if state_or_stage == UNKNOWN_U32 else state_or_stage,
                "update_epoch": None if epoch_or_iteration == UNKNOWN_U32 else epoch_or_iteration,
                "source_state": None if source_state_or_stage == UNKNOWN_U32 else source_state_or_stage,
                "source_update_epoch": None if source_epoch_or_iteration == UNKNOWN_U32 else source_epoch_or_iteration,
            }
            if version >= 6
            else {
                "transport_stage": None if state_or_stage == UNKNOWN_U32 else state_or_stage,
                "transport_iteration": None if epoch_or_iteration == UNKNOWN_U32 else epoch_or_iteration,
                "source_stage": None if source_state_or_stage == UNKNOWN_U32 else source_state_or_stage,
                "source_iteration": None if source_epoch_or_iteration == UNKNOWN_U32 else source_epoch_or_iteration,
            }
        )
        metadata = {
            "geometry_revision": None if geometry_revision == UNKNOWN_U32 else geometry_revision,
            "radiance_revision": None if radiance_revision == UNKNOWN_U32 else radiance_revision,
            "radiance_model_identity": None if radiance_model_identity == UNKNOWN_U64 else radiance_model_identity,
            "build_token_serial": None if build_token_serial == UNKNOWN_U64 else build_token_serial,
            "field_serial": None if field_serial == UNKNOWN_U64 else field_serial,
            "source_field_serial": None if source_field_serial == UNKNOWN_U64 else source_field_serial,
            "source_radiance_revision": None if source_radiance_revision == UNKNOWN_U32 else source_radiance_revision,
            "publication_state": None if publication_state == UNKNOWN_U32 else publication_state,
            "batch_order": None if batch_order == UNKNOWN_U32 else batch_order,
            "max_abs_delta": None if max_abs_delta == UNKNOWN_DELTA else max_abs_delta,
            "max_rel_delta": None if max_rel_delta == UNKNOWN_DELTA else max_rel_delta,
            "nonfinite_count": None if nonfinite_count == UNKNOWN_U32 else nonfinite_count,
            "valid_count": None if valid_count == UNKNOWN_U32 else valid_count,
            **lifecycle_metadata,
        }
    else:
        raise ValueError(f"{path}: unsupported version {version}")
    if channels != 4:
        raise ValueError(f"{path}: expected four float channels, got {channels}")
    expected_plane_counts = (
        (5,)
        if version == 8
        else ((4,) if version == 7 else ((3,) if version in (5, 6) else (1, 2)))
    )
    if plane_count not in expected_plane_counts:
        expected_label = (
            "five"
            if version == 8
            else (
                "four"
                if version == 7
                else ("three" if version in (5, 6) else "one or two")
            )
        )
        raise ValueError(
            f"{path}: expected {expected_label} float4 planes, got {plane_count}"
        )
    payload = data[header_size:]
    plane_size = width * height * PIXEL.size
    expected = plane_size * plane_count
    if len(payload) != expected:
        raise ValueError(f"{path}: payload is {len(payload)} bytes, expected {expected}")
    return Capture(
        path,
        version,
        width,
        height,
        backend,
        spacing,
        debug_view,
        payload[:plane_size],
        payload[plane_size : 2 * plane_size] if plane_count >= 2 else b"",
        payload[2 * plane_size : 3 * plane_size] if plane_count >= 3 else b"",
        payload[3 * plane_size : 4 * plane_size] if plane_count >= 4 else b"",
        payload[4 * plane_size : 5 * plane_size] if plane_count >= 5 else b"",
        plane_count,
        **metadata,
    )


def percentile(sorted_values: list[float], fraction: float) -> float:
    if not sorted_values:
        return 0.0
    index = math.ceil(fraction * len(sorted_values)) - 1
    return sorted_values[max(0, min(index, len(sorted_values) - 1))]


def position_in_world_roi(
    position: tuple[float, float, float],
    world_roi: tuple[float, float, float, float, float, float],
) -> bool:
    min_x, min_y, min_z, max_x, max_y, max_z = world_roi
    epsilon = WORLD_ROI_BOUNDARY_EPSILON
    return (
        min_x - epsilon <= position[0] <= max_x + epsilon
        and min_y - epsilon <= position[1] <= max_y + epsilon
        and min_z - epsilon <= position[2] <= max_z + epsilon
    )


def quantized_voxel_face_key(
    position: tuple[float, float, float],
) -> tuple[int, int, int, int] | None:
    scaled = tuple(value * TERRAIN_VOXELS_PER_WORLD_UNIT for value in position)
    face_axis = min(
        range(3), key=lambda axis: abs(scaled[axis] - round(scaled[axis]))
    )
    face_coordinate = round(scaled[face_axis])
    if abs(scaled[face_axis] - face_coordinate) > VOXEL_FACE_BOUNDARY_EPSILON:
        return None
    tangent_axes = tuple(axis for axis in range(3) if axis != face_axis)
    return (
        face_axis,
        face_coordinate,
        math.floor(scaled[tangent_axes[0]] + VOXEL_FACE_BOUNDARY_EPSILON),
        math.floor(scaled[tangent_axes[1]] + VOXEL_FACE_BOUNDARY_EPSILON),
    )


def receiver_voxel_key(
    position: tuple[float, float, float],
    camera_position: tuple[float, float, float],
) -> tuple[int, int, int] | None:
    camera_to_surface = tuple(
        position[axis] - camera_position[axis] for axis in range(3)
    )
    distance = math.sqrt(sum(value * value for value in camera_to_surface))
    if distance <= 1.0e-12:
        return None
    inward_direction = tuple(value / distance for value in camera_to_surface)
    return tuple(
        math.floor(
            position[axis] * TERRAIN_VOXELS_PER_WORLD_UNIT
            + inward_direction[axis] * RECEIVER_VOXEL_INTERIOR_NUDGE
        )
        for axis in range(3)
    )


def captured_receiver_voxel_key(
    center: tuple[float, float, float],
) -> tuple[int, int, int] | None:
    scaled_indices = tuple(
        value * TERRAIN_VOXELS_PER_WORLD_UNIT - 0.5 for value in center
    )
    rounded_indices = tuple(round(value) for value in scaled_indices)
    if any(
        abs(value - rounded) > CAPTURED_RECEIVER_CENTER_EPSILON
        for value, rounded in zip(scaled_indices, rounded_indices)
    ):
        return None
    return rounded_indices


def summarize(
    capture: Capture,
    world_roi: tuple[float, float, float, float, float, float] | None = None,
    direct_light_sunlit_roi: tuple[float, float, float, float, float, float]
    | None = None,
    direct_light_shadowed_roi: tuple[float, float, float, float, float, float]
    | None = None,
    camera_position: tuple[float, float, float] | None = None,
) -> dict[str, object]:
    luminances: list[float] = []
    finite = True
    terrain_hit_count = 0
    rgb_abs_max = 0.0
    rgb_nonzero_count = 0
    rgb_channel_min = [math.inf, math.inf, math.inf]
    rgb_channel_negative_count = [0, 0, 0]
    roi_terrain_hit_count = 0
    channel_abs_max = [0.0, 0.0, 0.0]
    channel_nonzero_count = [0, 0, 0]
    roi_channel_sum = [0.0, 0.0, 0.0]
    roi_luminances: list[float] = []
    roi_environment_zero_count = 0
    roi_environment_voxel_face_counts: dict[
        tuple[int, int, int, int], list[int]
    ] = {}
    roi_environment_receiver_voxel_counts: dict[tuple[int, int, int], list[int]] = {}
    world_min = [math.inf, math.inf, math.inf]
    world_max = [-math.inf, -math.inf, -math.inf]
    exact_sun_visibilities: list[float] = []
    world_pixels = (
        list(PIXEL.iter_unpack(capture.world_payload))
        if capture.world_payload
        else [None] * capture.sample_count
    )
    terrain_shadow_receiver_pixels = (
        list(PIXEL.iter_unpack(capture.terrain_shadow_receiver_payload))
        if capture.terrain_shadow_receiver_payload
        else [None] * capture.sample_count
    )
    direct_sun_shadow_pixels = (
        list(PIXEL.iter_unpack(capture.direct_sun_shadow_payload))
        if capture.direct_sun_shadow_payload
        else [None] * capture.sample_count
    )
    for (red, green, blue, hit), world_pixel in zip(
        PIXEL.iter_unpack(capture.payload), world_pixels
    ):
        rgb = (red, green, blue)
        finite_rgb = all(math.isfinite(value) for value in rgb)
        finite = finite and finite_rgb and math.isfinite(hit)
        position: tuple[float, float, float] | None = None
        exact_sun_visibility: float | None = None
        if world_pixel is not None:
            world_x, world_y, world_z, exact_sun_visibility = world_pixel
            position = (world_x, world_y, world_z)
            finite = finite and all(
                math.isfinite(value) for value in (*position, exact_sun_visibility)
            )
        if hit > 0.5:
            terrain_hit_count += 1
            if finite_rgb:
                luminances.append(0.2126 * red + 0.7152 * green + 0.0722 * blue)
                rgb_abs_max = max(rgb_abs_max, *(abs(value) for value in rgb))
                if any(value != 0.0 for value in rgb):
                    rgb_nonzero_count += 1
                for channel, value in enumerate(rgb):
                    rgb_channel_min[channel] = min(rgb_channel_min[channel], value)
                    if value < 0.0:
                        rgb_channel_negative_count[channel] += 1
            in_roi = world_roi is None
            if position is not None and world_roi is not None:
                in_roi = position_in_world_roi(position, world_roi)
            if in_roi:
                roi_terrain_hit_count += 1
                if finite_rgb:
                    roi_luminances.append(
                        0.2126 * red + 0.7152 * green + 0.0722 * blue
                    )
                    if all(value == 0.0 for value in rgb):
                        roi_environment_zero_count += 1
                    if position is not None:
                        face_key = quantized_voxel_face_key(position)
                        if face_key is not None:
                            counts = roi_environment_voxel_face_counts.setdefault(
                                face_key, [0, 0]
                            )
                            counts[
                                0 if all(value == 0.0 for value in rgb) else 1
                            ] += 1
                        if camera_position is not None:
                            voxel_key = receiver_voxel_key(position, camera_position)
                            if voxel_key is not None:
                                counts = roi_environment_receiver_voxel_counts.setdefault(
                                    voxel_key, [0, 0]
                                )
                                counts[
                                    0 if all(value == 0.0 for value in rgb) else 1
                                ] += 1
                    for channel, value in enumerate(rgb):
                        roi_channel_sum[channel] += value
                        channel_abs_max[channel] = max(
                            channel_abs_max[channel], abs(value)
                        )
                        if value != 0.0:
                            channel_nonzero_count[channel] += 1
                if position is not None:
                    for axis, value in enumerate(position):
                        world_min[axis] = min(world_min[axis], value)
                        world_max[axis] = max(world_max[axis], value)
                if exact_sun_visibility is not None and math.isfinite(
                    exact_sun_visibility
                ):
                    exact_sun_visibilities.append(exact_sun_visibility)

    direct_light_available = bool(capture.direct_light_payload)
    direct_light_finite = True
    direct_light_hit_mask_matches: bool | None = (
        True if direct_light_available else None
    )
    direct_light_luminances: list[float] = []
    direct_light_rgb_channel_negative_count = [0, 0, 0]
    roi_combined_zero_count = 0
    roi_combined_voxel_face_counts: dict[
        tuple[int, int, int, int], list[int]
    ] = {}
    roi_combined_receiver_voxel_counts: dict[tuple[int, int, int], list[int]] = {}
    terrain_shadow_receiver_available = bool(
        capture.terrain_shadow_receiver_payload
    )
    terrain_shadow_receiver_finite = True
    terrain_shadow_receiver_valid = True
    terrain_shadow_receiver_voxel_samples: dict[
        tuple[int, int, int], list[float]
    ] = {}
    direct_sun_shadow_available = bool(capture.direct_sun_shadow_payload)
    direct_sun_shadow_finite = True
    direct_sun_shadow_valid = True
    direct_sun_shadow_voxel_samples: dict[
        str, dict[tuple[int, int, int], list[float]]
    ] = {source: {} for source in ("terrain", "leaf", "cloud", "combined")}
    direct_light_roi_luminances = {
        "sunlit": [],
        "shadowed": [],
    }
    direct_light_roi_hit_counts = {
        "sunlit": 0,
        "shadowed": 0,
    }
    direct_light_rois = {
        "sunlit": direct_light_sunlit_roi,
        "shadowed": direct_light_shadowed_roi,
    }
    if direct_light_available:
        for (
            irradiance_pixel,
            world_pixel,
            direct_pixel,
            receiver_pixel,
            direct_sun_shadow_pixel,
        ) in zip(
            PIXEL.iter_unpack(capture.payload),
            world_pixels,
            PIXEL.iter_unpack(capture.direct_light_payload),
            terrain_shadow_receiver_pixels,
            direct_sun_shadow_pixels,
        ):
            terrain_hit = irradiance_pixel[3] > 0.5
            direct_red, direct_green, direct_blue, direct_hit = direct_pixel
            direct_rgb = (direct_red, direct_green, direct_blue)
            finite_direct_pixel = all(
                math.isfinite(value) for value in direct_pixel
            )
            direct_light_finite = direct_light_finite and finite_direct_pixel
            direct_light_hit_mask_matches = bool(
                direct_light_hit_mask_matches
                and terrain_hit == (direct_hit > 0.5)
            )
            if not terrain_hit or not finite_direct_pixel:
                continue
            direct_luminance = (
                0.2126 * direct_red
                + 0.7152 * direct_green
                + 0.0722 * direct_blue
            )
            direct_light_luminances.append(direct_luminance)
            captured_voxel_key = None
            if receiver_pixel is not None:
                receiver_center = receiver_pixel[:3]
                terrain_shadow_transmittance = receiver_pixel[3]
                finite_receiver = all(
                    math.isfinite(value) for value in receiver_pixel
                )
                terrain_shadow_receiver_finite = (
                    terrain_shadow_receiver_finite and finite_receiver
                )
                if finite_receiver:
                    captured_voxel_key = captured_receiver_voxel_key(receiver_center)
                    valid_receiver = (
                        captured_voxel_key is not None
                        and 0.0 <= terrain_shadow_transmittance <= 1.0
                    )
                    terrain_shadow_receiver_valid = (
                        terrain_shadow_receiver_valid and valid_receiver
                    )
                    if valid_receiver:
                        terrain_shadow_receiver_voxel_samples.setdefault(
                            captured_voxel_key, []
                        ).append(terrain_shadow_transmittance)
                else:
                    terrain_shadow_receiver_valid = False
            if direct_sun_shadow_pixel is not None:
                finite_shadows = all(
                    math.isfinite(value) for value in direct_sun_shadow_pixel
                )
                direct_sun_shadow_finite = (
                    direct_sun_shadow_finite and finite_shadows
                )
                terrain_shadow, leaf_shadow, cloud_shadow, combined_shadow = (
                    direct_sun_shadow_pixel
                )
                valid_shadows = (
                    finite_shadows
                    and captured_voxel_key is not None
                    and all(0.0 <= value <= 1.0 for value in direct_sun_shadow_pixel)
                    and abs(
                        combined_shadow
                        - terrain_shadow * leaf_shadow * cloud_shadow
                    )
                    <= 1.0e-5
                )
                direct_sun_shadow_valid = direct_sun_shadow_valid and valid_shadows
                if valid_shadows:
                    for source, value in zip(
                        ("terrain", "leaf", "cloud", "combined"),
                        direct_sun_shadow_pixel,
                    ):
                        direct_sun_shadow_voxel_samples[source].setdefault(
                            captured_voxel_key, []
                        ).append(value)
            for channel, value in enumerate(direct_rgb):
                if value < 0.0:
                    direct_light_rgb_channel_negative_count[channel] += 1
            in_world_roi = world_roi is None
            if world_pixel is not None and world_roi is not None:
                in_world_roi = position_in_world_roi(world_pixel[:3], world_roi)
            irradiance_rgb = irradiance_pixel[:3]
            combined_zero = all(
                irradiance + direct == 0.0
                for irradiance, direct in zip(irradiance_rgb, direct_rgb)
            )
            if (
                in_world_roi
                and all(math.isfinite(value) for value in irradiance_rgb)
                and combined_zero
            ):
                roi_combined_zero_count += 1
            if in_world_roi and world_pixel is not None:
                face_key = quantized_voxel_face_key(world_pixel[:3])
                if face_key is not None:
                    counts = roi_combined_voxel_face_counts.setdefault(
                        face_key, [0, 0]
                    )
                    counts[0 if combined_zero else 1] += 1
                if camera_position is not None:
                    voxel_key = receiver_voxel_key(world_pixel[:3], camera_position)
                    if voxel_key is not None:
                        counts = roi_combined_receiver_voxel_counts.setdefault(
                            voxel_key, [0, 0]
                        )
                        counts[0 if combined_zero else 1] += 1
            if world_pixel is None:
                continue
            position = world_pixel[:3]
            for roi_name, roi in direct_light_rois.items():
                if roi is None:
                    continue
                if position_in_world_roi(position, roi):
                    direct_light_roi_hit_counts[roi_name] += 1
                    direct_light_roi_luminances[roi_name].append(direct_luminance)

    luminances.sort()
    roi_luminances.sort()
    direct_light_luminances.sort()
    for values in direct_light_roi_luminances.values():
        values.sort()
    terrain_shadow_receiver_voxel_transmittance_ranges = sorted(
        max(samples) - min(samples)
        for samples in terrain_shadow_receiver_voxel_samples.values()
        if len(samples) >= DIRECT_LIGHT_RECEIVER_VOXEL_MIN_PIXELS
    )
    direct_sun_shadow_voxel_transmittance_ranges = {
        source: sorted(
            max(samples) - min(samples)
            for samples in voxel_samples.values()
            if len(samples) >= DIRECT_LIGHT_RECEIVER_VOXEL_MIN_PIXELS
        )
        for source, voxel_samples in direct_sun_shadow_voxel_samples.items()
    }
    roi_channel_mean = (
        [value / roi_terrain_hit_count for value in roi_channel_sum]
        if roi_terrain_hit_count > 0
        else None
    )
    roi_channel_advantage = (
        [
            value
            - max(
                other
                for other_index, other in enumerate(roi_channel_mean)
                if other_index != channel
            )
            for channel, value in enumerate(roi_channel_mean)
        ]
        if roi_channel_mean is not None
        else None
    )
    roi_channel_total = sum(roi_channel_mean) if roi_channel_mean is not None else 0.0
    roi_channel_share = (
        [value / roi_channel_total for value in roi_channel_mean]
        if roi_channel_mean is not None and roi_channel_total > 0.0
        else None
    )
    has_world_positions = world_min[0] != math.inf
    metadata_finite = all(
        value is None or math.isfinite(value)
        for value in (capture.max_abs_delta, capture.max_rel_delta)
    )
    return {
        "path": str(capture.path),
        "version": capture.version,
        "width": capture.width,
        "height": capture.height,
        "backend": capture.backend,
        "spacing_voxels": capture.spacing_voxels,
        "debug_view": DEBUG_VIEW_LABELS.get(capture.debug_view, capture.debug_view),
        "sample_count": capture.sample_count,
        "terrain_hit_count": terrain_hit_count,
        "finite": finite
        and direct_light_finite
        and terrain_shadow_receiver_finite
        and direct_sun_shadow_finite,
        "metadata_finite": metadata_finite,
        "rgb_abs_max": rgb_abs_max,
        "rgb_nonzero_count": rgb_nonzero_count,
        "rgb_channel_min": (
            rgb_channel_min if rgb_channel_min[0] != math.inf else None
        ),
        "rgb_channel_negative_count": rgb_channel_negative_count,
        "rgb_channel_abs_max": channel_abs_max,
        "rgb_channel_nonzero_count": channel_nonzero_count,
        "luminance_mean": sum(luminances) / len(luminances) if luminances else 0.0,
        "luminance_p99": percentile(luminances, 0.99),
        "luminance_max": luminances[-1] if luminances else 0.0,
        "world_roi": list(world_roi) if world_roi is not None else None,
        "world_roi_terrain_hit_count": roi_terrain_hit_count,
        "world_roi_rgb_channel_mean": roi_channel_mean,
        "world_roi_channel_advantage": roi_channel_advantage,
        "world_roi_channel_share": roi_channel_share,
        "world_roi_luminance_mean": (
            sum(roi_luminances) / len(roi_luminances) if roi_luminances else None
        ),
        "world_roi_luminance_p99": (
            percentile(roi_luminances, 0.99) if roi_luminances else None
        ),
        "world_roi_luminance_max": (
            roi_luminances[-1] if roi_luminances else None
        ),
        "world_roi_environment_zero_count": roi_environment_zero_count,
        "world_roi_quantized_voxel_face_count": len(
            roi_environment_voxel_face_counts
        ),
        "world_roi_mixed_environment_zero_voxel_face_count": sum(
            min(counts) >= VOXEL_FACE_MIN_MIXED_CLASS_PIXELS
            for counts in roi_environment_voxel_face_counts.values()
        ),
        "world_roi_receiver_voxel_count": (
            len(roi_environment_receiver_voxel_counts)
            if camera_position is not None
            else None
        ),
        "world_roi_mixed_environment_zero_receiver_voxel_count": (
            sum(
                min(counts) >= VOXEL_FACE_MIN_MIXED_CLASS_PIXELS
                for counts in roi_environment_receiver_voxel_counts.values()
            )
            if camera_position is not None
            else None
        ),
        "world_roi_combined_zero_count": (
            roi_combined_zero_count if direct_light_available else None
        ),
        "world_roi_mixed_combined_zero_voxel_face_count": (
            sum(
                min(counts) >= VOXEL_FACE_MIN_MIXED_CLASS_PIXELS
                for counts in roi_combined_voxel_face_counts.values()
            )
            if direct_light_available
            else None
        ),
        "world_roi_mixed_combined_zero_receiver_voxel_count": (
            sum(
                min(counts) >= VOXEL_FACE_MIN_MIXED_CLASS_PIXELS
                for counts in roi_combined_receiver_voxel_counts.values()
            )
            if direct_light_available and camera_position is not None
            else None
        ),
        "world_position_min": world_min if has_world_positions else None,
        "world_position_max": world_max if has_world_positions else None,
        "exact_direct_sun_visibility_mean": (
            sum(exact_sun_visibilities) / len(exact_sun_visibilities)
            if exact_sun_visibilities
            else None
        ),
        "exact_direct_sun_visibility_min": (
            min(exact_sun_visibilities) if exact_sun_visibilities else None
        ),
        "exact_direct_sun_visibility_max": (
            max(exact_sun_visibilities) if exact_sun_visibilities else None
        ),
        "direct_light_available": direct_light_available,
        "direct_light_finite": direct_light_finite if direct_light_available else None,
        "direct_light_hit_mask_matches": direct_light_hit_mask_matches,
        "direct_light_rgb_channel_negative_count": (
            direct_light_rgb_channel_negative_count
            if direct_light_available
            else None
        ),
        "direct_light_luminance_mean": (
            sum(direct_light_luminances) / len(direct_light_luminances)
            if direct_light_luminances
            else None
        ),
        "direct_light_luminance_p99": (
            percentile(direct_light_luminances, 0.99)
            if direct_light_luminances
            else None
        ),
        "direct_light_luminance_max": (
            direct_light_luminances[-1] if direct_light_luminances else None
        ),
        "terrain_shadow_receiver_available": terrain_shadow_receiver_available,
        "terrain_shadow_receiver_finite": (
            terrain_shadow_receiver_finite
            if terrain_shadow_receiver_available
            else None
        ),
        "terrain_shadow_receiver_valid": (
            terrain_shadow_receiver_valid
            if terrain_shadow_receiver_available
            else None
        ),
        "terrain_shadow_receiver_voxel_count": (
            len(terrain_shadow_receiver_voxel_transmittance_ranges)
            if terrain_shadow_receiver_available
            else None
        ),
        "terrain_shadow_receiver_voxel_transmittance_range_p99": (
            percentile(
                terrain_shadow_receiver_voxel_transmittance_ranges, 0.99
            )
            if terrain_shadow_receiver_voxel_transmittance_ranges
            else None
        ),
        "terrain_shadow_receiver_voxel_transmittance_range_max": (
            terrain_shadow_receiver_voxel_transmittance_ranges[-1]
            if terrain_shadow_receiver_voxel_transmittance_ranges
            else None
        ),
        "direct_sun_shadow_available": direct_sun_shadow_available,
        "direct_sun_shadow_finite": (
            direct_sun_shadow_finite if direct_sun_shadow_available else None
        ),
        "direct_sun_shadow_valid": (
            direct_sun_shadow_valid if direct_sun_shadow_available else None
        ),
        **{
            f"{source}_shadow_receiver_voxel_count": (
                len(ranges) if direct_sun_shadow_available else None
            )
            for source, ranges in direct_sun_shadow_voxel_transmittance_ranges.items()
            if source != "terrain"
        },
        **{
            f"{source}_shadow_receiver_voxel_transmittance_range_p99": (
                percentile(ranges, 0.99) if ranges else None
            )
            for source, ranges in direct_sun_shadow_voxel_transmittance_ranges.items()
            if source != "terrain"
        },
        **{
            f"{source}_shadow_receiver_voxel_transmittance_range_max": (
                ranges[-1] if ranges else None
            )
            for source, ranges in direct_sun_shadow_voxel_transmittance_ranges.items()
            if source != "terrain"
        },
        "direct_light_sunlit_roi": (
            list(direct_light_sunlit_roi)
            if direct_light_sunlit_roi is not None
            else None
        ),
        "direct_light_sunlit_roi_terrain_hit_count": direct_light_roi_hit_counts[
            "sunlit"
        ],
        "direct_light_sunlit_roi_luminance_mean": (
            sum(direct_light_roi_luminances["sunlit"])
            / len(direct_light_roi_luminances["sunlit"])
            if direct_light_roi_luminances["sunlit"]
            else None
        ),
        "direct_light_sunlit_roi_luminance_p99": (
            percentile(direct_light_roi_luminances["sunlit"], 0.99)
            if direct_light_roi_luminances["sunlit"]
            else None
        ),
        "direct_light_sunlit_roi_luminance_max": (
            direct_light_roi_luminances["sunlit"][-1]
            if direct_light_roi_luminances["sunlit"]
            else None
        ),
        "direct_light_shadowed_roi": (
            list(direct_light_shadowed_roi)
            if direct_light_shadowed_roi is not None
            else None
        ),
        "direct_light_shadowed_roi_terrain_hit_count": direct_light_roi_hit_counts[
            "shadowed"
        ],
        "direct_light_shadowed_roi_luminance_mean": (
            sum(direct_light_roi_luminances["shadowed"])
            / len(direct_light_roi_luminances["shadowed"])
            if direct_light_roi_luminances["shadowed"]
            else None
        ),
        "direct_light_shadowed_roi_luminance_p99": (
            percentile(direct_light_roi_luminances["shadowed"], 0.99)
            if direct_light_roi_luminances["shadowed"]
            else None
        ),
        "direct_light_shadowed_roi_luminance_max": (
            direct_light_roi_luminances["shadowed"][-1]
            if direct_light_roi_luminances["shadowed"]
            else None
        ),
        "geometry_revision": capture.geometry_revision,
        "radiance_revision": capture.radiance_revision,
        "radiance_model_identity": capture.radiance_model_identity,
        "token_serial": capture.build_token_serial,
        "build_token_serial": capture.build_token_serial,
        "field_serial": capture.field_serial,
        "lifecycle_state": LIFECYCLE_STATE_LABELS.get(
            capture.lifecycle_state, capture.lifecycle_state
        ),
        "update_epoch": capture.update_epoch,
        "source_state": LIFECYCLE_STATE_LABELS.get(
            capture.source_state, capture.source_state
        ),
        "source_update_epoch": capture.source_update_epoch,
        "transport_stage": TRANSPORT_STAGE_LABELS.get(
            capture.transport_stage, capture.transport_stage
        ),
        "transport_iteration": capture.transport_iteration,
        "source_stage": TRANSPORT_STAGE_LABELS.get(
            capture.source_stage, capture.source_stage
        ),
        "source_iteration": capture.source_iteration,
        "source_identity": capture.source_identity,
        "source_field_serial": capture.source_field_serial,
        "source_radiance_revision": capture.source_radiance_revision,
        "publication_state": PUBLICATION_STATE_LABELS.get(
            capture.publication_state, capture.publication_state
        ),
        "batch_order": BATCH_ORDER_LABELS.get(
            capture.batch_order, capture.batch_order
        ),
        "max_abs_delta": capture.max_abs_delta,
        "max_rel_delta": capture.max_rel_delta,
        "header_nonfinite_count": capture.nonfinite_count,
        "header_valid_count": capture.valid_count,
        "payload_sha256": hashlib.sha256(capture.payload).hexdigest(),
        "direct_light_payload_sha256": (
            hashlib.sha256(capture.direct_light_payload).hexdigest()
            if direct_light_available
            else None
        ),
        "terrain_shadow_receiver_payload_sha256": (
            hashlib.sha256(capture.terrain_shadow_receiver_payload).hexdigest()
            if terrain_shadow_receiver_available
            else None
        ),
        "direct_sun_shadow_payload_sha256": (
            hashlib.sha256(capture.direct_sun_shadow_payload).hexdigest()
            if direct_sun_shadow_available
            else None
        ),
    }


def metadata_mismatches(first: Capture, second: Capture) -> list[str]:
    if first.version < 3 and second.version < 3:
        return []
    fields = [
        "version",
        "plane_count",
        "geometry_revision",
        "radiance_revision",
        "radiance_model_identity",
        "build_token_serial",
        "publication_state",
    ]
    if first.version >= 6 or second.version >= 6:
        fields.extend(
            ["lifecycle_state", "update_epoch", "source_state", "source_update_epoch"]
        )
    else:
        fields.extend(
            ["transport_stage", "transport_iteration", "source_stage", "source_iteration"]
        )
    if first.version >= 4 or second.version >= 4:
        fields.extend(
            [
                "field_serial",
                "source_field_serial",
                "source_radiance_revision",
            ]
        )
    else:
        fields.append("source_identity")
    return [
        field for field in fields if getattr(first, field) != getattr(second, field)
    ]


def cross_process_metadata_mismatches(
    first: Capture, second: Capture
) -> tuple[list[str], list[str]]:
    mismatches = metadata_mismatches(first, second)
    ignored: list[str] = []
    if first.version >= 6 and second.version >= 6:
        process_local_fields = {
            "build_token_serial",
            "field_serial",
            "source_field_serial",
        }
        ignored = [field for field in mismatches if field in process_local_fields]
        mismatches = [field for field in mismatches if field not in process_local_fields]
    return mismatches, ignored


def compare(first: Capture, second: Capture) -> dict[str, object]:
    base_compatible = (
        first.width,
        first.height,
        first.backend,
        first.spacing_voxels,
        first.debug_view,
    ) == (
        second.width,
        second.height,
        second.backend,
        second.spacing_voxels,
        second.debug_view,
    )
    mismatches, process_local_identity_mismatches = cross_process_metadata_mismatches(
        first, second
    )
    compatible = base_compatible and not mismatches
    environment_bit_exact = (
        compatible
        and first.payload == second.payload
        and first.world_payload == second.world_payload
        and first.terrain_shadow_receiver_payload
        == second.terrain_shadow_receiver_payload
        and first.direct_sun_shadow_payload == second.direct_sun_shadow_payload
    )
    direct_light_bit_exact = (
        compatible and first.direct_light_payload == second.direct_light_payload
    )
    return {
        "compatible": compatible,
        "metadata_mismatches": mismatches,
        "process_local_identity_mismatches": process_local_identity_mismatches,
        "environment_bit_exact": environment_bit_exact,
        "direct_light_bit_exact": direct_light_bit_exact,
        "bit_exact": environment_bit_exact and direct_light_bit_exact,
        "first_sha256": hashlib.sha256(first.payload).hexdigest(),
        "second_sha256": hashlib.sha256(second.payload).hexdigest(),
        "first_direct_light_sha256": (
            hashlib.sha256(first.direct_light_payload).hexdigest()
            if first.direct_light_payload
            else None
        ),
        "second_direct_light_sha256": (
            hashlib.sha256(second.direct_light_payload).hexdigest()
            if second.direct_light_payload
            else None
        ),
        "first_terrain_shadow_receiver_sha256": (
            hashlib.sha256(first.terrain_shadow_receiver_payload).hexdigest()
            if first.terrain_shadow_receiver_payload
            else None
        ),
        "second_terrain_shadow_receiver_sha256": (
            hashlib.sha256(second.terrain_shadow_receiver_payload).hexdigest()
            if second.terrain_shadow_receiver_payload
            else None
        ),
        "first_direct_sun_shadow_sha256": (
            hashlib.sha256(first.direct_sun_shadow_payload).hexdigest()
            if first.direct_sun_shadow_payload
            else None
        ),
        "second_direct_sun_shadow_sha256": (
            hashlib.sha256(second.direct_sun_shadow_payload).hexdigest()
            if second.direct_sun_shadow_payload
            else None
        ),
    }


def world_xyz_payload(payload: bytes) -> bytes:
    return b"".join(
        pixel[:12]
        for pixel in (
            payload[index : index + PIXEL.size]
            for index in range(0, len(payload), PIXEL.size)
        )
    )


def float4_alpha_payload(payload: bytes) -> bytes:
    return b"".join(
        pixel[12:]
        for pixel in (
            payload[index : index + PIXEL.size]
            for index in range(0, len(payload), PIXEL.size)
        )
    )


def compare_radiance_frame(current: Capture, baseline: Capture) -> dict[str, object]:
    base_compatible = (
        current.version,
        current.width,
        current.height,
        current.backend,
        current.spacing_voxels,
        current.debug_view,
    ) == (
        baseline.version,
        baseline.width,
        baseline.height,
        baseline.backend,
        baseline.spacing_voxels,
        baseline.debug_view,
    )
    mismatches = metadata_mismatches(current, baseline)
    environment_payload_bit_exact = current.payload == baseline.payload
    terrain_hit_mask_bit_exact = float4_alpha_payload(
        current.payload
    ) == float4_alpha_payload(baseline.payload)
    world_xyz_bit_exact = world_xyz_payload(current.world_payload) == world_xyz_payload(
        baseline.world_payload
    )
    exact_sun_visibility_bit_exact = float4_alpha_payload(
        current.world_payload
    ) == float4_alpha_payload(baseline.world_payload)
    receiver_center_xyz_bit_exact = world_xyz_payload(
        current.terrain_shadow_receiver_payload
    ) == world_xyz_payload(baseline.terrain_shadow_receiver_payload)
    terrain_shadow_transmittance_bit_exact = float4_alpha_payload(
        current.terrain_shadow_receiver_payload
    ) == float4_alpha_payload(baseline.terrain_shadow_receiver_payload)
    compatible = base_compatible and not mismatches
    return {
        "compatible": compatible,
        "metadata_mismatches": mismatches,
        "environment_payload_bit_exact": environment_payload_bit_exact,
        "world_xyz_bit_exact": world_xyz_bit_exact,
        "terrain_hit_mask_bit_exact": terrain_hit_mask_bit_exact,
        "exact_sun_visibility_bit_exact": exact_sun_visibility_bit_exact,
        "receiver_center_xyz_bit_exact": receiver_center_xyz_bit_exact,
        "terrain_shadow_transmittance_bit_exact": terrain_shadow_transmittance_bit_exact,
    }


def compare_reference(approximate: Capture, exact: Capture) -> dict[str, object]:
    base_compatible = (
        approximate.width,
        approximate.height,
        approximate.backend,
        approximate.spacing_voxels,
    ) == (
        exact.width,
        exact.height,
        exact.backend,
        exact.spacing_voxels,
    )
    mismatches, process_local_identity_mismatches = cross_process_metadata_mismatches(
        approximate, exact
    )
    compatible = base_compatible and not mismatches
    if not compatible:
        return {
            "compatible": False,
            "metadata_mismatches": mismatches,
            "process_local_identity_mismatches": process_local_identity_mismatches,
        }

    luminance_errors: list[float] = []
    luminance_overestimates: list[float] = []
    channel_errors: list[float] = []
    hit_mask_matches = True
    peak_error = (-1.0, 0, 0)
    peak_overestimate = (0.0, 0, 0)
    for index, (approx_pixel, exact_pixel) in enumerate(
        zip(PIXEL.iter_unpack(approximate.payload), PIXEL.iter_unpack(exact.payload))
    ):
        ar, ag, ab, ah = approx_pixel
        er, eg, eb, eh = exact_pixel
        hit_mask_matches = hit_mask_matches and ((ah > 0.5) == (eh > 0.5))
        if ah <= 0.5 or eh <= 0.5:
            continue
        rgb_error = (abs(ar - er), abs(ag - eg), abs(ab - eb))
        luminance_error = (
            0.2126 * rgb_error[0] + 0.7152 * rgb_error[1] + 0.0722 * rgb_error[2]
        )
        approximate_luminance = 0.2126 * ar + 0.7152 * ag + 0.0722 * ab
        exact_luminance = 0.2126 * er + 0.7152 * eg + 0.0722 * eb
        overestimate = max(0.0, approximate_luminance - exact_luminance)
        x = index % approximate.width
        y = index // approximate.width
        if luminance_error > peak_error[0]:
            peak_error = (luminance_error, x, y)
        if overestimate > peak_overestimate[0]:
            peak_overestimate = (overestimate, x, y)
        luminance_errors.append(luminance_error)
        luminance_overestimates.append(overestimate)
        channel_errors.append(max(rgb_error))
    luminance_errors.sort()
    luminance_overestimates.sort()
    channel_errors.sort()
    return {
        "compatible": True,
        "metadata_mismatches": [],
        "process_local_identity_mismatches": process_local_identity_mismatches,
        "hit_mask_matches": hit_mask_matches,
        "sample_count": len(luminance_errors),
        "luminance_error_mean": (
            sum(luminance_errors) / len(luminance_errors) if luminance_errors else 0.0
        ),
        "luminance_error_p99": percentile(luminance_errors, 0.99),
        "luminance_error_max": luminance_errors[-1] if luminance_errors else 0.0,
        "luminance_error_peak_xy": [peak_error[1], peak_error[2]],
        "luminance_overestimate_mean": (
            sum(luminance_overestimates) / len(luminance_overestimates)
            if luminance_overestimates else 0.0
        ),
        "luminance_overestimate_p99": percentile(luminance_overestimates, 0.99),
        "luminance_overestimate_max": (
            luminance_overestimates[-1] if luminance_overestimates else 0.0
        ),
        "luminance_overestimate_peak_xy": [
            peak_overestimate[1], peak_overestimate[2]
        ],
        "channel_error_p99": percentile(channel_errors, 0.99),
        "channel_error_max": channel_errors[-1] if channel_errors else 0.0,
    }


def compare_roi_baseline(
    current: Capture,
    baseline: Capture,
    world_roi: tuple[float, float, float, float, float, float] | None,
) -> dict[str, object]:
    metadata_fields = (
        "geometry_revision",
        "radiance_revision",
        "radiance_model_identity",
    )
    metadata_mismatches = [
        field
        for field in metadata_fields
        if getattr(current, field) != getattr(baseline, field)
    ]
    base_compatible = (
        current.width,
        current.height,
        current.backend,
        current.spacing_voxels,
        current.debug_view,
    ) == (
        baseline.width,
        baseline.height,
        baseline.backend,
        baseline.spacing_voxels,
        baseline.debug_view,
    )
    if current.world_payload != baseline.world_payload:
        metadata_mismatches.append("world_payload")
    current_summary = summarize(current, world_roi)
    baseline_summary = summarize(baseline, world_roi)
    current_mean = current_summary["world_roi_luminance_mean"]
    baseline_mean = baseline_summary["world_roi_luminance_mean"]
    current_channel_share = current_summary["world_roi_channel_share"]
    baseline_channel_share = baseline_summary["world_roi_channel_share"]
    compatible = (
        base_compatible
        and not metadata_mismatches
        and current_mean is not None
        and baseline_mean is not None
    )
    return {
        "compatible": compatible,
        "metadata_mismatches": metadata_mismatches,
        "baseline_roi_luminance_mean": baseline_mean,
        "current_roi_luminance_mean": current_mean,
        "roi_luminance_gain": (
            current_mean - baseline_mean if compatible else None
        ),
        "baseline_roi_channel_share": baseline_channel_share,
        "current_roi_channel_share": current_channel_share,
        "roi_channel_share_gain": (
            [
                current - baseline
                for current, baseline in zip(
                    current_channel_share, baseline_channel_share
                )
            ]
            if compatible
            and current_channel_share is not None
            and baseline_channel_share is not None
            else None
        ),
    }


def compare_debug_baseline(
    current: Capture,
    baseline: Capture,
    world_roi: tuple[float, float, float, float, float, float] | None,
) -> dict[str, object]:
    mismatches, process_local_identity_mismatches = cross_process_metadata_mismatches(
        current, baseline
    )
    base_compatible = (
        current.version,
        current.width,
        current.height,
        current.backend,
        current.spacing_voxels,
    ) == (
        baseline.version,
        baseline.width,
        baseline.height,
        baseline.backend,
        baseline.spacing_voxels,
    )
    world_xyz_matches = world_xyz_payload(
        current.world_payload
    ) == world_xyz_payload(baseline.world_payload)
    terrain_hit_mask_matches = float4_alpha_payload(
        current.payload
    ) == float4_alpha_payload(baseline.payload)
    current_summary = summarize(current, world_roi)
    baseline_summary = summarize(baseline, world_roi)
    current_mean = current_summary["world_roi_luminance_mean"]
    baseline_mean = baseline_summary["world_roi_luminance_mean"]
    compatible = (
        base_compatible
        and not mismatches
        and current.debug_view != baseline.debug_view
        and world_xyz_matches
        and terrain_hit_mask_matches
        and current_mean is not None
        and baseline_mean is not None
    )
    return {
        "compatible": compatible,
        "metadata_mismatches": mismatches,
        "process_local_identity_mismatches": process_local_identity_mismatches,
        "baseline_debug_view": DEBUG_VIEW_LABELS.get(
            baseline.debug_view, baseline.debug_view
        ),
        "current_debug_view": DEBUG_VIEW_LABELS.get(
            current.debug_view, current.debug_view
        ),
        "world_xyz_matches": world_xyz_matches,
        "terrain_hit_mask_matches": terrain_hit_mask_matches,
        "baseline_roi_luminance_mean": baseline_mean,
        "current_roi_luminance_mean": current_mean,
        "roi_luminance_gain": (
            current_mean - baseline_mean if compatible else None
        ),
    }


def compare_direct_light_baseline(
    current: Capture,
    baseline: Capture,
    sunlit_roi: tuple[float, float, float, float, float, float] | None,
) -> dict[str, object]:
    current_summary = summarize(current, direct_light_sunlit_roi=sunlit_roi)
    baseline_summary = summarize(baseline, direct_light_sunlit_roi=sunlit_roi)
    current_mean = current_summary["direct_light_sunlit_roi_luminance_mean"]
    baseline_mean = baseline_summary["direct_light_sunlit_roi_luminance_mean"]
    current_hits = current_summary["direct_light_sunlit_roi_terrain_hit_count"]
    baseline_hits = baseline_summary["direct_light_sunlit_roi_terrain_hit_count"]
    compatible = (
        current.version >= 5
        and current.version == baseline.version
        and current.width == baseline.width
        and current.height == baseline.height
        and current.backend == baseline.backend
        and current.spacing_voxels == baseline.spacing_voxels
        and world_xyz_payload(current.world_payload)
        == world_xyz_payload(baseline.world_payload)
        and current_hits == baseline_hits
        and current_hits > 0
        and current_mean is not None
        and baseline_mean is not None
    )
    delta = current_mean - baseline_mean if compatible else None
    return {
        "compatible": compatible,
        "sunlit_roi": list(sunlit_roi) if sunlit_roi is not None else None,
        "sunlit_roi_terrain_hit_count": current_hits if compatible else None,
        "baseline_sunlit_roi_luminance_mean": baseline_mean,
        "current_sunlit_roi_luminance_mean": current_mean,
        "sunlit_roi_luminance_delta": delta,
        "sunlit_roi_luminance_absolute_delta": abs(delta) if delta is not None else None,
        "baseline_direct_light_sha256": hashlib.sha256(
            baseline.direct_light_payload
        ).hexdigest(),
        "current_direct_light_sha256": hashlib.sha256(
            current.direct_light_payload
        ).hexdigest(),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("capture", type=Path)
    parser.add_argument("--compare", type=Path)
    parser.add_argument("--radiance-frame-baseline", type=Path)
    parser.add_argument(
        "--compare-direct-light",
        action="store_true",
        help="also require the optional direct-light plane to be bit-exact",
    )
    parser.add_argument("--reference", type=Path)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--debug-baseline", type=Path)
    parser.add_argument("--direct-light-baseline", type=Path)
    parser.add_argument(
        "--min-direct-light-sunlit-roi-luminance-absolute-delta", type=float
    )
    parser.add_argument("--min-roi-luminance-gain", type=float)
    parser.add_argument("--max-luminance", type=float)
    parser.add_argument("--require-zero-rgb", action="store_true")
    parser.add_argument("--require-nonnegative-rgb", action="store_true")
    parser.add_argument("--min-luminance-p99", type=float)
    parser.add_argument("--max-reference-error-p99", type=float)
    parser.add_argument("--min-reference-error-p99", type=float)
    parser.add_argument("--max-reference-overestimate-p99", type=float)
    parser.add_argument("--world-roi", type=float, nargs=6)
    parser.add_argument("--camera-position", type=float, nargs=3)
    parser.add_argument(
        "--roi-channel", choices=tuple(ROI_CHANNEL_INDICES), default="red"
    )
    parser.add_argument("--min-roi-channel-advantage", type=float)
    parser.add_argument("--max-roi-channel-advantage", type=float)
    parser.add_argument("--max-roi-channel-share", type=float)
    parser.add_argument("--min-roi-channel-share-gain", type=float)
    parser.add_argument("--min-roi-luminance-mean", type=float)
    parser.add_argument("--max-roi-luminance-mean", type=float)
    parser.add_argument("--min-debug-roi-luminance-gain", type=float)
    parser.add_argument("--max-debug-roi-luminance-gain", type=float)
    parser.add_argument("--max-world-roi-environment-zero-count", type=int)
    parser.add_argument("--max-world-roi-combined-zero-count", type=int)
    parser.add_argument(
        "--max-world-roi-mixed-environment-zero-voxel-face-count", type=int
    )
    parser.add_argument(
        "--max-world-roi-mixed-combined-zero-voxel-face-count", type=int
    )
    parser.add_argument(
        "--max-world-roi-mixed-environment-zero-receiver-voxel-count", type=int
    )
    parser.add_argument(
        "--max-world-roi-mixed-combined-zero-receiver-voxel-count", type=int
    )
    parser.add_argument("--max-exact-direct-sun-visibility", type=float)
    parser.add_argument("--direct-light-sunlit-roi", type=float, nargs=6)
    parser.add_argument("--min-direct-light-sunlit-luminance-mean", type=float)
    parser.add_argument("--direct-light-shadowed-roi", type=float, nargs=6)
    parser.add_argument("--max-direct-light-shadowed-luminance-max", type=float)
    parser.add_argument(
        "--max-terrain-shadow-receiver-voxel-transmittance-range",
        type=float,
        help=(
            "require captured marcher voxels with at least four internal pixels "
            "to have no more than this terrain VSM transmittance range"
        ),
    )
    parser.add_argument(
        "--max-leaf-shadow-receiver-voxel-transmittance-range",
        type=float,
        help=(
            "require captured marcher voxels with at least four internal pixels "
            "to have no more than this leaf-shadow transmittance range"
        ),
    )
    parser.add_argument(
        "--max-combined-shadow-receiver-voxel-transmittance-range",
        type=float,
        help=(
            "require captured marcher voxels with at least four internal pixels "
            "to have no more than this combined direct-shadow transmittance range"
        ),
    )
    parser.add_argument("--expect-version", type=int)
    parser.add_argument(
        "--expect-debug-view", choices=tuple(DEBUG_VIEW_LABELS.values())
    )
    parser.add_argument("--expect-spacing-voxels", type=int)
    parser.add_argument("--expect-geometry-revision", type=int)
    parser.add_argument("--expect-radiance-revision", type=int)
    parser.add_argument("--expect-build-token-serial", type=int)
    parser.add_argument("--expect-field-serial", type=int)
    parser.add_argument(
        "--expect-lifecycle-state", choices=tuple(LIFECYCLE_STATE_LABELS.values())
    )
    parser.add_argument("--expect-update-epoch", type=int)
    parser.add_argument(
        "--expect-source-state", choices=tuple(LIFECYCLE_STATE_LABELS.values())
    )
    parser.add_argument("--expect-source-update-epoch", type=int)
    parser.add_argument(
        "--expect-transport-stage", choices=tuple(TRANSPORT_STAGE_LABELS.values())
    )
    parser.add_argument("--expect-transport-iteration", type=int)
    parser.add_argument(
        "--expect-source-stage", choices=tuple(TRANSPORT_STAGE_LABELS.values())
    )
    parser.add_argument("--expect-source-iteration", type=int)
    parser.add_argument("--expect-source-identity", type=int)
    parser.add_argument("--expect-source-field-serial", type=int)
    parser.add_argument("--expect-source-radiance-revision", type=int)
    parser.add_argument(
        "--expect-publication-state", choices=tuple(PUBLICATION_STATE_LABELS.values())
    )
    parser.add_argument(
        "--expect-batch-order", choices=tuple(BATCH_ORDER_LABELS.values())
    )
    parser.add_argument("--convergence-max-abs-delta", type=float)
    parser.add_argument("--convergence-max-rel-delta", type=float)
    parser.add_argument(
        "--correctness",
        action="store_true",
        help="apply correctness-gate policy, including rejecting NonConverged fields",
    )
    args = parser.parse_args()

    first = load_capture(args.capture)
    capture_summary = summarize(
        first,
        tuple(args.world_roi) if args.world_roi is not None else None,
        (
            tuple(args.direct_light_sunlit_roi)
            if args.direct_light_sunlit_roi is not None
            else None
        ),
        (
            tuple(args.direct_light_shadowed_roi)
            if args.direct_light_shadowed_roi is not None
            else None
        ),
        tuple(args.camera_position) if args.camera_position is not None else None,
    )
    failures: list[str] = []
    report: dict[str, object] = {
        "capture": capture_summary,
        "validation_failures": failures,
    }
    exit_code = 0

    def expect(field: str, expected: object) -> None:
        if expected is None:
            return
        actual = capture_summary[field]
        if actual != expected:
            failures.append(f"{field}: expected {expected}, got {actual}")

    expect("version", args.expect_version)
    expect("debug_view", args.expect_debug_view)
    expect("spacing_voxels", args.expect_spacing_voxels)
    expect("geometry_revision", args.expect_geometry_revision)
    expect("radiance_revision", args.expect_radiance_revision)
    expect("build_token_serial", args.expect_build_token_serial)
    expect("field_serial", args.expect_field_serial)
    expect("lifecycle_state", args.expect_lifecycle_state)
    expect("update_epoch", args.expect_update_epoch)
    expect("source_state", args.expect_source_state)
    expect("source_update_epoch", args.expect_source_update_epoch)
    expect("transport_stage", args.expect_transport_stage)
    expect("transport_iteration", args.expect_transport_iteration)
    expect("source_stage", args.expect_source_stage)
    expect("source_iteration", args.expect_source_iteration)
    expect("source_identity", args.expect_source_identity)
    expect("source_field_serial", args.expect_source_field_serial)
    expect("source_radiance_revision", args.expect_source_radiance_revision)
    expect("publication_state", args.expect_publication_state)
    expect("batch_order", args.expect_batch_order)

    def gate_min(field: str, threshold: float | None) -> None:
        nonlocal exit_code
        if threshold is None:
            return
        actual = capture_summary[field]
        if actual is None or actual < threshold:
            failures.append(f"{field}: expected at least {threshold:g}, got {actual}")
            exit_code = 1

    def gate_max(field: str, threshold: float | None) -> None:
        nonlocal exit_code
        if threshold is None:
            return
        actual = capture_summary[field]
        if actual is None or actual > threshold:
            failures.append(f"{field}: expected at most {threshold:g}, got {actual}")
            exit_code = 1

    roi_advantages = capture_summary["world_roi_channel_advantage"]
    selected_advantage = (
        roi_advantages[ROI_CHANNEL_INDICES[args.roi_channel]]
        if roi_advantages is not None
        else None
    )
    capture_summary["selected_roi_channel"] = args.roi_channel
    capture_summary["selected_roi_channel_advantage"] = selected_advantage
    roi_channel_shares = capture_summary["world_roi_channel_share"]
    selected_channel_share = (
        roi_channel_shares[ROI_CHANNEL_INDICES[args.roi_channel]]
        if roi_channel_shares is not None
        else None
    )
    capture_summary["selected_roi_channel_share"] = selected_channel_share
    gate_min("selected_roi_channel_advantage", args.min_roi_channel_advantage)
    gate_max("selected_roi_channel_advantage", args.max_roi_channel_advantage)
    gate_max("selected_roi_channel_share", args.max_roi_channel_share)
    gate_min("world_roi_luminance_mean", args.min_roi_luminance_mean)
    gate_max("world_roi_luminance_mean", args.max_roi_luminance_mean)
    gate_max(
        "world_roi_environment_zero_count",
        args.max_world_roi_environment_zero_count,
    )
    gate_max(
        "world_roi_combined_zero_count",
        args.max_world_roi_combined_zero_count,
    )
    gate_max(
        "world_roi_mixed_environment_zero_voxel_face_count",
        args.max_world_roi_mixed_environment_zero_voxel_face_count,
    )
    gate_max(
        "world_roi_mixed_combined_zero_voxel_face_count",
        args.max_world_roi_mixed_combined_zero_voxel_face_count,
    )
    gate_max(
        "world_roi_mixed_environment_zero_receiver_voxel_count",
        args.max_world_roi_mixed_environment_zero_receiver_voxel_count,
    )
    gate_max(
        "world_roi_mixed_combined_zero_receiver_voxel_count",
        args.max_world_roi_mixed_combined_zero_receiver_voxel_count,
    )
    gate_max(
        "exact_direct_sun_visibility_max",
        args.max_exact_direct_sun_visibility,
    )
    gate_min(
        "direct_light_sunlit_roi_luminance_mean",
        args.min_direct_light_sunlit_luminance_mean,
    )
    gate_max(
        "direct_light_shadowed_roi_luminance_max",
        args.max_direct_light_shadowed_luminance_max,
    )
    gate_max(
        "terrain_shadow_receiver_voxel_transmittance_range_max",
        args.max_terrain_shadow_receiver_voxel_transmittance_range,
    )
    gate_max(
        "leaf_shadow_receiver_voxel_transmittance_range_max",
        args.max_leaf_shadow_receiver_voxel_transmittance_range,
    )
    gate_max(
        "combined_shadow_receiver_voxel_transmittance_range_max",
        args.max_combined_shadow_receiver_voxel_transmittance_range,
    )
    if first.nonfinite_count is not None:
        expect("header_nonfinite_count", 0)
    if not capture_summary["metadata_finite"]:
        failures.append("capture metadata contains nonfinite convergence values")
    if args.correctness and capture_summary["transport_stage"] == "non-converged":
        failures.append("correctness mode rejects NonConverged DDGI fields")
    if capture_summary["direct_light_available"]:
        if not capture_summary["direct_light_hit_mask_matches"]:
            failures.append("direct-light plane hit mask does not match irradiance plane")
        if args.correctness and any(
            count != 0
            for count in capture_summary["direct_light_rgb_channel_negative_count"]
        ):
            failures.append("terrain-hit direct-light RGB contains negative channels")
    if capture_summary["terrain_shadow_receiver_available"]:
        if not capture_summary["terrain_shadow_receiver_valid"]:
            failures.append(
                "terrain-shadow receiver plane contains a noncanonical center or "
                "transmittance outside [0, 1]"
            )
    if capture_summary["direct_sun_shadow_available"]:
        if not capture_summary["direct_sun_shadow_valid"]:
            failures.append(
                "direct-sun shadow plane contains a noncanonical receiver, a "
                "transmittance outside [0, 1], or an invalid combined product"
            )
    if capture_summary["transport_stage"] == "converged":
        for field, threshold in (
            ("max_abs_delta", args.convergence_max_abs_delta),
            ("max_rel_delta", args.convergence_max_rel_delta),
        ):
            if threshold is None:
                continue
            actual = capture_summary[field]
            if actual is None:
                failures.append(
                    f"{field}: converged value is unknown; threshold is {threshold:g}"
                )
            elif actual > threshold:
                failures.append(
                    f"{field}: converged value {actual:g} exceeds {threshold:g}"
                )
    if failures:
        exit_code = 1
    if args.compare is not None:
        comparison = compare(first, load_capture(args.compare))
        report["comparison"] = comparison
        if not comparison["environment_bit_exact"]:
            exit_code = 1
        if args.compare_direct_light and not comparison["direct_light_bit_exact"]:
            exit_code = 1
    elif args.compare_direct_light:
        failures.append("--compare-direct-light requires --compare")
        exit_code = 1
    if args.radiance_frame_baseline is not None:
        radiance_frame = compare_radiance_frame(
            first, load_capture(args.radiance_frame_baseline)
        )
        report["radiance_frame_comparison"] = radiance_frame
        if not (
            radiance_frame["compatible"]
            and radiance_frame["environment_payload_bit_exact"]
            and radiance_frame["world_xyz_bit_exact"]
            and radiance_frame["terrain_hit_mask_bit_exact"]
        ):
            failures.append(
                "radiance frame changed active DDGI irradiance, world XYZ, or terrain hit mask"
            )
            exit_code = 1
    if args.direct_light_baseline is not None:
        direct_baseline = compare_direct_light_baseline(
            first,
            load_capture(args.direct_light_baseline),
            (
                tuple(args.direct_light_sunlit_roi)
                if args.direct_light_sunlit_roi is not None
                else None
            ),
        )
        report["direct_light_baseline_comparison"] = direct_baseline
        if not direct_baseline["compatible"]:
            failures.append("direct-light baseline comparison is incompatible")
            exit_code = 1
        threshold = args.min_direct_light_sunlit_roi_luminance_absolute_delta
        actual = direct_baseline["sunlit_roi_luminance_absolute_delta"]
        if threshold is not None and (actual is None or actual < threshold):
            failures.append(
                "direct-light sunlit ROI luminance absolute delta: "
                f"expected at least {threshold:g}, got {actual}"
            )
            exit_code = 1
    elif args.min_direct_light_sunlit_roi_luminance_absolute_delta is not None:
        failures.append(
            "--min-direct-light-sunlit-roi-luminance-absolute-delta requires "
            "--direct-light-baseline"
        )
        exit_code = 1
    if args.reference is not None:
        reference = compare_reference(first, load_capture(args.reference))
        report["reference_comparison"] = reference
        if not reference["compatible"] or not reference.get("hit_mask_matches", False):
            exit_code = 1
        if (
            args.min_reference_error_p99 is not None
            and reference.get("luminance_error_p99", -math.inf)
            < args.min_reference_error_p99
        ):
            failures.append(
                "reference luminance_error_p99: expected at least "
                f"{args.min_reference_error_p99:g}, got "
                f"{reference.get('luminance_error_p99')}"
            )
            exit_code = 1
        if (
            args.max_reference_error_p99 is not None
            and reference.get("luminance_error_p99", math.inf)
            > args.max_reference_error_p99
        ):
            failures.append(
                "reference luminance_error_p99: expected at most "
                f"{args.max_reference_error_p99:g}, got "
                f"{reference.get('luminance_error_p99'):g}"
            )
            exit_code = 1
        if (
            args.max_reference_overestimate_p99 is not None
            and reference.get("luminance_overestimate_p99", math.inf)
            > args.max_reference_overestimate_p99
        ):
            failures.append(
                "reference luminance_overestimate_p99: expected at most "
                f"{args.max_reference_overestimate_p99:g}, got "
                f"{reference.get('luminance_overestimate_p99'):g}"
            )
            exit_code = 1
    elif args.min_reference_error_p99 is not None:
        failures.append("--min-reference-error-p99 requires --reference")
        exit_code = 1
    if args.debug_baseline is not None:
        debug_baseline = compare_debug_baseline(
            first,
            load_capture(args.debug_baseline),
            tuple(args.world_roi) if args.world_roi is not None else None,
        )
        report["debug_baseline_comparison"] = debug_baseline
        if not debug_baseline["compatible"]:
            failures.append("debug baseline comparison is incompatible")
            exit_code = 1
        gain = debug_baseline["roi_luminance_gain"]
        if args.min_debug_roi_luminance_gain is not None and (
            gain is None or gain < args.min_debug_roi_luminance_gain
        ):
            failures.append(
                "debug roi_luminance_gain: expected at least "
                f"{args.min_debug_roi_luminance_gain:g}, got {gain}"
            )
            exit_code = 1
        if args.max_debug_roi_luminance_gain is not None and (
            gain is None or gain > args.max_debug_roi_luminance_gain
        ):
            failures.append(
                "debug roi_luminance_gain: expected at most "
                f"{args.max_debug_roi_luminance_gain:g}, got {gain}"
            )
            exit_code = 1
    elif (
        args.min_debug_roi_luminance_gain is not None
        or args.max_debug_roi_luminance_gain is not None
    ):
        failures.append("debug ROI luminance gates require --debug-baseline")
        exit_code = 1
    if args.baseline is not None:
        baseline_comparison = compare_roi_baseline(
            first,
            load_capture(args.baseline),
            tuple(args.world_roi) if args.world_roi is not None else None,
        )
        report["baseline_comparison"] = baseline_comparison
        if not baseline_comparison["compatible"]:
            failures.append("ROI baseline capture is incompatible")
            exit_code = 1
        else:
            if (
                args.min_roi_luminance_gain is not None
                and baseline_comparison["roi_luminance_gain"]
                < args.min_roi_luminance_gain
            ):
                failures.append(
                    "roi_luminance_gain: expected at least "
                    f"{args.min_roi_luminance_gain:g}, got "
                    f"{baseline_comparison['roi_luminance_gain']}"
                )
                exit_code = 1
            share_gains = baseline_comparison["roi_channel_share_gain"]
            if share_gains is not None:
                selected_share_gain = share_gains[
                    ROI_CHANNEL_INDICES[args.roi_channel]
                ]
                baseline_comparison["selected_roi_channel"] = args.roi_channel
                baseline_comparison["selected_roi_channel_share_gain"] = (
                    selected_share_gain
                )
            else:
                selected_share_gain = None
            if args.min_roi_channel_share_gain is not None and (
                selected_share_gain is None
                or selected_share_gain < args.min_roi_channel_share_gain
            ):
                failures.append(
                    "selected_roi_channel_share_gain: expected at least "
                    f"{args.min_roi_channel_share_gain:g}, got {selected_share_gain}"
                )
                exit_code = 1
    elif (
        args.min_roi_luminance_gain is not None
        or args.min_roi_channel_share_gain is not None
    ):
        failures.append("ROI gain gates require --baseline")
        exit_code = 1
    if not capture_summary["finite"]:
        failures.append("payload contains nonfinite values")
        exit_code = 1
    if capture_summary["terrain_hit_count"] == 0:
        failures.append("capture contains no terrain hits")
        exit_code = 1
    if (
        args.max_luminance is not None
        and capture_summary["luminance_max"] > args.max_luminance
    ):
        exit_code = 1
    if args.require_zero_rgb and capture_summary["rgb_nonzero_count"] != 0:
        exit_code = 1
    if args.require_nonnegative_rgb and any(
        count != 0 for count in capture_summary["rgb_channel_negative_count"]
    ):
        failures.append("terrain-hit RGB contains negative channel values")
        exit_code = 1
    if (
        args.min_luminance_p99 is not None
        and capture_summary["luminance_p99"] < args.min_luminance_p99
    ):
        exit_code = 1
    print(json.dumps(report, indent=2, sort_keys=True))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
