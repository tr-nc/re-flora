// Event-only gather from the last published attached pose. One dispatch per
// batch, never one queue wait per leaf. Shared vertex functions own appearance.
import flora_vertex;
import flora_types;
import tree_scene;
import vegetation_response_sample;
import vegetation_response_types;
import flora_motion;
import leaf_flutter;
import leaf_model_pose;

struct LeafHandoffInput {
    uint4 world; // xyz voxel root; w packed local offset from twig
    uint4 state; // x absolute response state index; y floatBits(growth)
};
struct LeafHandoffOutput {
    float4 position_size;
    float4 velocity;
    float4 normal;
    float4 color;
    float4 rotation;
    float4 angular_velocity;
};
struct B_LeafHandoffInputs { LeafHandoffInput data[]; };
struct B_LeafHandoffOutputs { LeafHandoffOutput data[]; };
[[vk::binding(0, 1)]] readonly GLSLShaderStorageBuffer<B_LeafHandoffInputs, Std430DataLayout> leaf_handoff_inputs;
[[vk::binding(1, 1)]] GLSLShaderStorageBuffer<B_LeafHandoffOutputs, Std430DataLayout> leaf_handoff_outputs;

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 tid : SV_DispatchThreadID)
{
    PushConstantFlora pc = floraPushConstants();
    if (tid.x >= floraLightingCacheInstanceCount(pc)) return;
    LeafHandoffInput input = leaf_handoff_inputs.data[tid.x];
    int3 local = unpackTreeLeafVoxelLocalPosition(input.world.w);
    uint3 anchor = uint3(int3(input.world.xyz) - local);
    LeafModelPose pose = attachedLeafModelPose(input.world.xyz,input.world.w,input.state.x,asfloat(input.state.y));
    uint leafSeed = pose.seed;
    LeafHandoffOutput output;
    output.position_size = float4(pose.center, pose.size);
    output.color = pose.color;
    output.normal = float4(leafQuaternionVector(pose.rotation,float3(0,0,1)),asfloat(pose.seed));
    output.rotation = pose.rotation;
    output.velocity = float4(0.0);
    output.angular_velocity = float4(0.0);
    if (attachedVegetationResponseEnabled())
    {
        VegetationResponseState state = leafHandoffResponse(input.state.x);
        float2 velocity = state.held[windVolumeBucketIndex(leafSeed)].zw;
        float3 localVelocity = float3(velocity.x, 0.0, velocity.y) * max(gui_input.leaf_global_offset_scale, 0.0);
        float angle = clamp(state.torsion.x, -1.0, 1.0);
        float speed = abs(state.torsion.x) <= 1.0 ? state.torsion.z : 0.0;
        if (gui_input.leaf_flutter_strength > 0.0)
        {
            float h = 0.001;
            localVelocity += (leafLocalDisplacement(leafSeed, angle + speed*h, gui_input.leaf_local_displacement_voxels)
                - leafLocalDisplacement(leafSeed, angle - speed*h, gui_input.leaf_local_displacement_voxels)) / (2.0*h);
        }
        output.velocity.xyz = treeAttachmentVector(anchor, localVelocity / 256.0);
        if(gui_input.attached_leaf_rotation!=0u)
            output.angular_velocity.xyz = leafHinge(leafSeed) * speed;
    }
    leaf_handoff_outputs.data[tid.x] = output;
}
