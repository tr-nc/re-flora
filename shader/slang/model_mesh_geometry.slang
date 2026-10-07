// Unlit immutable world-model geometry; shared by color and depth-only adapters.
// This seam deliberately has no environment/DDGI or shadow-map sampling bindings.
module model_mesh_geometry;
import model_bake_triangle;
[[vk::binding(16,1)]] public StructuredBuffer<ButterflyMeshTriangle> model_mesh_triangles;
[[vk::binding(17,1)]] public StructuredBuffer<uint4> model_mesh_ranges;
public float3 meshPoint(ButterflyMeshTriangle t,uint corner) {
    return t.a.xyz+(corner==1u?t.e1.xyz:corner==2u?t.e2.xyz:float3(0));
}
public float3 meshNormal(ButterflyMeshTriangle t,uint corner) {
    return corner==0u?t.normal0.xyz:corner==1u?t.normal1.xyz:t.normal2.xyz;
}
