import wind_field;
import tree_skin;

struct PoseJoint { float4 start_frequency; float4 end_compliance; uint4 parent; };
struct PoseState { float4 rotation; float4 translation; float4 angle; float4 velocity; };
[[vk::binding(0,0)]] StructuredBuffer<PoseJoint> tree_pose_joints;
[[vk::binding(1,0)]] RWStructuredBuffer<PoseState> tree_pose_state;
[[vk::binding(2,0)]] StructuredBuffer<uint4> tree_pose_ranges;
[[vk::binding(3,0)]] StructuredBuffer<uint4> tree_pose_step;

float4 multiplyRotation(float4 a, float4 b) {
    return float4(a.w*b.xyz + b.w*a.xyz + cross(a.xyz,b.xyz), a.w*b.w-dot(a.xyz,b.xyz));
}
float4 scaledAxisRotation(float3 angle) {
    float thetaSquared = dot(angle, angle);
    if (thetaSquared < 1e-6) return float4(angle*(0.5-thetaSquared/48.0), 1.0-thetaSquared/8.0);
    float theta = sqrt(thetaSquared);
    return float4(angle * (sin(theta*0.5)/theta), cos(theta*0.5));
}

// One invocation owns a complete tree hierarchy. Parents precede children,
// including within each integration substep; separate trees execute in parallel.
// There is no fixed-depth approximation or cross-workgroup dependency.
[shader("compute")]
[numthreads(64,1,1)]
void main(uint3 id : SV_DispatchThreadID) {
    uint4 step = tree_pose_step[0];
    if (id.x >= step.x) return;
    float dt = min(asfloat(step.y), 0.25);
    if (dt <= 0.0) return;
    uint substeps = uint(max(ceil(dt*120.0), 1.0));
    float h = dt/float(substeps);
    uint2 range = tree_pose_ranges[id.x].xy;
    for (uint substep = 0; substep < substeps; ++substep) {
        for (uint local = 0; local < range.y; ++local) {
            uint index = range.x + local;
            PoseJoint joint = tree_pose_joints[index];
            PoseState state = tree_pose_state[index];
            float4 parentRotation = float4(0,0,0,1);
            float3 parentTranslation = float3(0);
            if (joint.parent.x != 0xffffffffu) {
                PoseState parent = tree_pose_state[range.x+joint.parent.x];
                parentRotation = parent.rotation;
                parentTranslation = parent.translation.xyz;
            }
            float3 start = joint.start_frequency.xyz;
            float3 end = joint.end_compliance.xyz;
            float3 sample = treeRotate(state.rotation, (start+end)*0.5) + state.translation.xyz;
            float2 windXZ = authoredWindEnabled() ? sampleAuthoredWind(sample.xz*256.0) : float2(0);
            float3 localWind = treeRotate(float4(-parentRotation.xyz, parentRotation.w), float3(windXZ.x,0,windXZ.y));
            float3 axis = end-start;
            axis = dot(axis,axis) > 0.0 ? normalize(axis) : float3(0);
            float3 target = cross(axis,localWind)*joint.end_compliance.w;
            float targetLength = length(target);
            if (targetLength > 0.22) target *= 0.22/targetLength;
            float omega = 6.283185307179586 * joint.start_frequency.w;
            float decay = 0.38*omega;
            float oscillation = omega*sqrt(1.0-0.38*0.38);
            float3 displacement = state.angle.xyz-target;
            float c = cos(oscillation*h);
            float s = sin(oscillation*h)/oscillation;
            float envelope = exp(-decay*h);
            float3 angle = target + envelope*(displacement*c+(state.velocity.xyz+decay*displacement)*s);
            float3 velocity = envelope*(state.velocity.xyz*c-(decay*state.velocity.xyz+omega*omega*displacement)*s);
            state.rotation = normalize(multiplyRotation(parentRotation, scaledAxisRotation(angle)));
            float3 pivot = treeRotate(parentRotation,start)+parentTranslation;
            state.translation = float4(pivot-treeRotate(state.rotation,start),0);
            state.angle = float4(angle,0);
            state.velocity = float4(velocity,0);
            tree_pose_state[index] = state;
        }
    }
}
