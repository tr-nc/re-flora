module vegetation_response_types;

public struct VegetationResponseState
{
    public float4 motion; // displacement xz, velocity xz, in wind/voxel units
    public float4 held[4]; // published displacement + velocity; no temporal interpolation
    public float4 torsion; // angle x, unused y, angular velocity z, unused w
    public float2 flutter_phase; // fractional noise coordinates, always [0, 1)
    public uint2 flutter_cells; // integer noise coordinates, retained on remap
};

public struct VegetationResponseInput
{
    public float4 root;
    public uint4 identity; // previous dense index, species, stable motion seed, individual-leaf flag
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
    public float4 controls; // mechanical frequency, damping, force gain multipliers, leaf flutter gain
    public float4 flutter_curve; // wind onset, full response, knee bias, low excitation (0..2)
    public float4 flutter_frequency; // low Hz, high Hz, overall scale, reserved
    public float4 flutter_frequency_curve; // wind onset, full response, knee bias, reserved
    public float4 grass_amplitude; // low/high tip voxels, wind onset/full
    public float4 grass_frequency; // low/high Hz, wind onset/full
    public float4 grass_curve; // amplitude/frequency knee, reserved
};
