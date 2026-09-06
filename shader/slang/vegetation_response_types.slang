module vegetation_response_types;

public struct VegetationResponseState
{
    public float4 motion; // displacement xz, velocity xz, in wind/voxel units
    public float4 held[4]; // published displacement + velocity; no temporal interpolation
};

public struct VegetationResponseInput
{
    public float4 root;
    public uint4 identity; // previous dense index, species, reserved, reserved
};

public struct U_VegetationResponseInfo
{
    public float4 grid; // world origin xz, spacing, attached-field count
    public uint4 shape; // width, depth, enabled, state count
};

public struct VegetationResponsePushConstants
{
    public float start_time;
    public float end_time;
    public float tick_seconds;
    public uint count;
    public float4 controls; // mechanical frequency, damping, force gain multipliers, reserved
};
