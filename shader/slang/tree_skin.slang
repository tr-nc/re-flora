module tree_skin;

// Shared by the resident surface producer and its GPU contract test.
public float3 treeRotate(float4 q, float3 v) {
    return v + 2.0 * cross(q.xyz, cross(q.xyz, v) + q.w * v);
}
public float3 treeSkinVector(float4 parent, float4 child, float weight, float3 v) {
    return lerp(treeRotate(parent, v), treeRotate(child, v), weight);
}
public float3 treeSkinPoint(float4 parent, float3 parentTranslation,
    float4 child, float3 childTranslation, float weight, float3 p) {
    return treeSkinVector(parent, child, weight, p)
        + lerp(parentTranslation, childTranslation, weight);
}
public float3 treeSkinNormal(float4 parent, float4 child, float weight, float3 n) {
    float3 x = treeSkinVector(parent, child, weight, float3(1,0,0));
    float3 y = treeSkinVector(parent, child, weight, float3(0,1,0));
    float3 z = treeSkinVector(parent, child, weight, float3(0,0,1));
    // Inverse transpose of the blended linear map, not a blended rotation.
    float determinant = dot(x, cross(y,z));
    float3 transformed = (cross(y,z)*n.x + cross(z,x)*n.y + cross(x,y)*n.z) / determinant;
    return dot(transformed, transformed) > 0.0 ? normalize(transformed) : float3(0);
}
public float3 treeCubeCorner(uint i) {
    // Same order as voxel_geometry::VOXEL_VERTICES; covered by GPU tests.
    return float3((i == 1u || i == 2u || i == 5u || i == 6u) ? 1.0 : 0.0,
        i >= 4u ? 1.0 : 0.0, (i % 4u >= 2u) ? 1.0 : 0.0);
}
