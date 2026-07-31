import ddgi_octahedral;

static const uint DDGI_INTERIOR_SIDE = 16u;
static const uint DDGI_STORED_SIDE = 18u;
static const uint DDGI_GUTTER_WORKGROUP_SIZE = 64u;

[[vk::binding(28, 0)]]
RWTexture2D<float2> ddgi_visibility_atlas;

struct DdgiAtlasGutterPushConstants
{
    uint first_probe_index;
    uint probe_count;
    uint tile_columns;
    uint padding;
};

[[vk::push_constant]]
ConstantBuffer<DdgiAtlasGutterPushConstants> pc;

[shader("compute")]
[numthreads(DDGI_GUTTER_WORKGROUP_SIZE, 1, 1)]
void main(uint3 groupId : SV_GroupID,
          uint3 localInvocationId : SV_GroupThreadID)
{
    uint localProbeIndex = groupId.x;
    if (localProbeIndex >= pc.probe_count) return;
    uint probeIndex = pc.first_probe_index + localProbeIndex;
    uint2 tileOrigin =
        ddgiTileOrigin(probeIndex, pc.tile_columns, DDGI_STORED_SIDE);
    uint texelCount = DDGI_STORED_SIDE * DDGI_STORED_SIDE;
    for (uint texel = localInvocationId.x; texel < texelCount;
         texel += DDGI_GUTTER_WORKGROUP_SIZE)
    {
        uint2 destination =
            uint2(texel % DDGI_STORED_SIDE, texel / DDGI_STORED_SIDE);
        uint last = DDGI_STORED_SIDE - 1u;
        bool isBorder = destination.x == 0u || destination.x == last ||
                        destination.y == 0u || destination.y == last;
        if (isBorder)
        {
            ddgi_visibility_atlas[tileOrigin + destination] =
                ddgi_visibility_atlas[
                    tileOrigin +
                    ddgiGutterSource(destination, DDGI_INTERIOR_SIDE)];
        }
    }
}
