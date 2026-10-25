import vegetation_water_solver;
import vegetation_water_types;
import tree_scene;
[[vk::binding(0,1)]] StructuredBuffer<WaterResponseInput> water_inputs;
[[vk::binding(1,1)]] StructuredBuffer<WaterResponseState> water_previous;
[[vk::binding(2,1)]] RWStructuredBuffer<WaterResponseState> water_output;
[[vk::binding(3,1)]] StructuredBuffer<VegetationWaterLobe> water_spray;
[[vk::binding(4,1)]] StructuredBuffer<float4> water_spray_bounds; // three float4 per emitter
[[vk::push_constant]] ConstantBuffer<WaterResponseStep> pc;
[shader("compute")]
[numthreads(64,1,1)]
void main(uint3 tid:SV_DispatchThreadID) {
    if(tid.x>=pc.count)return;
    WaterResponseInput input=water_inputs[tid.x];
    uint previous=pc.remap!=0?input.history.x:tid.x;
    WaterResponseState state=(WaterResponseState)0;
    if(pc.reset==0 && previous!=0xffffffffu)state=water_previous[previous];
    float3 position=(float3(input.key.xyz)+float3(0.5))/256.0;
    if(input.key.w<2u)position.y+=(input.key.w==0?2.5:1.0)/256.0;
    bool attached=input.key.w>=8u;
    if(attached)position=treeAttachmentPoint(input.anchor.xyz,position);
    float4 water=float4(0);
    for(uint source=0;source<pc.sources;++source) {
        if(any(position<water_spray_bounds[source*3].xyz) ||
           any(position>water_spray_bounds[source*3+1].xyz))continue;
        for(uint lobe=0;lobe<pc.lobes;++lobe)
            water+=waterLobeInput(water_spray[source*pc.lobes+lobe],position);
    }
    if(attached) {
        float4 rotation=treeAttachmentRotation(input.anchor.xyz);
        rotation.xyz=-rotation.xyz;
        water.xyz+=2.0*cross(rotation.xyz,cross(rotation.xyz,water.xyz)+rotation.w*water.xyz);
    }
    water_output[tid.x]=pc.enabled!=0?
        advanceWaterResponse(state,water,pc.dt,input.key.w,input.history.y):(WaterResponseState)0;
}
