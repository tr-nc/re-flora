import tracer_shadowing;

bool same(float3 a, float3 b) { return all(abs(a - b) < float3(1.0e-6)); }

export __extern_cpp int main()
{
    float3 center = float3(1, 2, 3);
    float3 normal = float3(0, 1, 0);
    float offset = 0.0065;
    DirectSunShadowReceiver voxel = voxelSunShadowReceiver(center, normal, offset);
    if (!same(voxel.terrain_world_position.xyz, center + normal * (0.5 / 256.0 + offset))) return 1;
    if (!same(voxel.leaf_world_position.xyz, voxel.terrain_world_position.xyz)) return 2;
    float3 face = center + normal * (0.5 / 256.0);
    DirectSunShadowReceiver surface = surfaceSunShadowReceiver(face, normal, offset);
    if (!same(surface.terrain_world_position.xyz, voxel.terrain_world_position.xyz)) return 3;
    if (!same(surface.receiver_plane_normal, normal)) return 4;
    // Grass terrain follows its posed point; leaves remain at the rest anchor.
    float3 posed = center + float3(0.01, 0, 0);
    DirectSunShadowReceiver grass = withLeafReceiver(pointSunShadowReceiver(posed), center);
    if (!same(grass.terrain_world_position.xyz, posed) || !same(grass.leaf_world_position.xyz, center)) return 5;
    if ((grass.flags & DIRECT_SUN_SHADOW_FLAG_LEAF_DEPTH_GATE) == 0u) return 6;
    DirectSunShadowReceiver masked = pointSunShadowReceiver(center,
        DIRECT_SUN_SHADOW_SOURCE_ALL, DIRECT_SUN_SHADOW_SOURCE_LEAF);
    if (masked.source_mask != 3u || masked.available_mask != 2u) return 7;
    DirectSunShadowReceiver zeroNormal = voxelSunShadowReceiver(center, float3(0), offset);
    if ((zeroNormal.flags & DIRECT_SUN_SHADOW_FLAG_RECEIVER_PLANE) != 0u) return 8;
    // A leaf in front of the receiver blocks; a receiver at/in front of the
    // premultiplied caster depth does not. No-depth-gate policy stays explicit.
    float4 leaf = float4(0.2, 0, 0, 0.5); // caster depth .4
    if (leafShadowSampleOpacity(leaf, 0.6, true) != 0.5) return 9;
    if (leafShadowSampleOpacity(leaf, 0.401, true) != 0.0) return 10;
    if (leafShadowSampleOpacity(leaf, 0.2, true) != 0.0) return 11;
    if (leafShadowSampleOpacity(leaf, 0.2, false) != 0.5) return 12;
    if (leafShadowSampleOpacity(float4(0), 0.6, true) != 0.0) return 13;
    return 0;
}
