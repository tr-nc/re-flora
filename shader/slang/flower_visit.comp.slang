// Sparse flower contacts use exactly the same growth/spawn/wind/assembly pose
// as flower heads. No independent CPU wind solver or root-height approximation.
import flower_model;
import flower_visit_types;
import model_pixel_types;
[[vk::binding(26,1)]] StructuredBuffer<uint4> flower_visit_requests;
[[vk::binding(27,1)]] RWStructuredBuffer<FlowerVisitPose> flower_visit_output;
[shader("compute")]
[numthreads(32,1,1)]
void main(uint index:SV_DispatchThreadID) {
    if(index>=flower_pc.prepare_object)return;
    uint slot=flower_pc.tile_first+index;
    uint4 request=flower_visit_requests[slot];
    FlowerPlantPose plant=flowerPlantPose(request.x);
    FlowerModelPart head=flowerPart(1u+request.y);
    FlowerVisitPose result;
    result.position_radius=float4(modelPixelWorld(plant.frame,head.landing.xyz),
        head.center_radius.w*plant.frame.scale);
    result.normal_growth=float4(flowerNormal(plant.frame,head.landing_normal.xyz),
        plant.frame.scale/max(flower_pc.world_scale,1e-7));
    float3 tangent=plant.frame.axisX-result.normal_growth.xyz*dot(plant.frame.axisX,result.normal_growth.xyz);
    if(length(tangent)<1e-5)tangent=plant.frame.axisZ-result.normal_growth.xyz*dot(plant.frame.axisZ,result.normal_growth.xyz);
    result.tangent_valid=float4(normalize(tangent),
        head.landing.w>0.5 && result.normal_growth.w>=0.6 && plant.frame.scale>1e-7?1.0:0.0);
    flower_visit_output[slot]=result;
}
