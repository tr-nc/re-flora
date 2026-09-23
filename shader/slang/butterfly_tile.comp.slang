import particle_model_shading;
import butterfly_mesh_types;
[[vk::binding(74,0)]] RWStructuredBuffer<float4> butterfly_pixel_tiles;

[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id : SV_DispatchThreadID) {
    ButterflyMeshInstance instance=butterfly_mesh_instances[id.z];
    if (any(id.xy>=uint2(instance.metadata.z))) return;
    butterfly_pixel_tiles[butterflyTileAddress(id.z,id.xy)]=sampleParticleModel(instance,id.xy);
}
