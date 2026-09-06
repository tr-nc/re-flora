import vegetation_response_types;
import wind_volume_sample;
import vegetation_response_solver;

struct B_ManualResponseInputs { VegetationResponseInput data[]; };
struct B_ManualResponsePrevious { VegetationResponseState data[]; };
struct B_ManualResponseOutput { VegetationResponseState data[]; };

[[vk::binding(0, 1)]] readonly GLSLShaderStorageBuffer<B_ManualResponseInputs, Std430DataLayout> response_inputs;
[[vk::binding(1, 1)]] readonly GLSLShaderStorageBuffer<B_ManualResponsePrevious, Std430DataLayout> response_previous;
[[vk::binding(2, 1)]] GLSLShaderStorageBuffer<B_ManualResponseOutput, Std430DataLayout> response_output;
[[vk::push_constant]] ConstantBuffer<VegetationResponsePushConstants> pc;

[shader("compute")]
[numthreads(64, 1, 1)]
void main(uint3 tid : SV_DispatchThreadID)
{
    if (tid.x >= pc.count) return;
    VegetationResponseInput input = response_inputs.data[tid.x];
    VegetationResponseState state = (VegetationResponseState)0;
    if (input.identity.x != 0xffffffffu)
        state = response_previous.data[input.identity.x];

    float time = pc.start_time;
    float tick = max(pc.tick_seconds, 1.0 / 240.0);
    uint endPublication = uint(floor(pc.end_time / tick + 0.00001));
    uint publication = max(uint(floor(time / tick + 0.00001)) + 1u,
                           endPublication > 3u ? endPublication - 3u : 1u);
    // Long loading/edit hitches retain displacement and velocity. Exact steps
    // remain stable with a coarser force sample; only the last four published
    // poses can still be observed, so do not replay discarded pose buckets.
    float maxStep = max(1.0 / 120.0, (pc.end_time - pc.start_time) / 64.0);
    for (uint step = 0u; step < 128u && time < pc.end_time; ++step)
    {
        float boundary = float(publication) * tick;
        float end = min(pc.end_time, min(time + maxStep, boundary));
        float dt = max(end - time, 0.0);
        float2 force = sampleProceduralWind(input.root.xyz, time + 0.5 * dt).xz;
        advanceVegetationResponse(state.motion, force, dt, input.identity.y, pc.controls);
        time = end;
        if (time >= boundary - 0.000001)
        {
            state.held[publication % 4u] = state.motion;
            ++publication;
        }
    }
    response_output.data[tid.x] = state;
}
