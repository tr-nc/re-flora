module grass_visibility_bounds;
import flora_types;
import tracer_types;

// Conservative full-growth envelope, in voxels. This is not a current-height
// test: growth, mowing, spawn and wilt cannot invalidate the admission decision.
// prepareFloraVertex: band y <= h; natural bend has horizontal/vertical magnitude
// <= h (arc angle <= 1.45); wilt length < .25h; vibration <= 1.45*abs(amplitude).
// Spawn is a convex interpolation of -(h+1), overshoot and zero. Midpoint rings
// stay inside the envelope; end extension + square radius + center offset < 2.
public float3 grassVisibilityExtent(uint bands, float3 wind, U_GuiInput gui_input, U_FloraGrowthInfo flora_growth_info) {
    float h = float(bands);
    float spawn = max(h + 1.0, max(abs(flora_growth_info.spawn_overshoot_min_voxels),
                                 abs(flora_growth_info.spawn_overshoot_max_voxels)));
    float vibration = 1.45 * abs(gui_input.grass_vibration_amplitude_voxels);
    return (float3(1.25*h + vibration + 2.0, 2.25*h + spawn + 2.0,
                   1.25*h + vibration + 2.0) + abs(wind)) / 256.0;
}

bool outsidePlane(float4 plane, float3 center, float3 extent) {
    // NaN/overflow fail open. Never reject a near-plane crossing or camera-inside box.
    return dot(plane, float4(center, 1.0)) + dot(abs(plane.xyz), extent) < -1e-5;
}
public bool grassBoundsVisible(float4x4 m, float3 center, float3 extent) {
    // Slang indexes mathematical rows. Vulkan clip depth is [0,w], not [-w,w].
    return !(outsidePlane(m[3]+m[0], center, extent) ||
             outsidePlane(m[3]-m[0], center, extent) ||
             outsidePlane(m[3]+m[1], center, extent) ||
             outsidePlane(m[3]-m[1], center, extent) ||
             outsidePlane(m[2], center, extent) ||
             outsidePlane(m[3]-m[2], center, extent));
}
