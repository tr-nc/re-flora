module ddgi_types;

public static const uint DDGI_PROBE_STATE_INACTIVE = 0u;
public static const uint DDGI_PROBE_STATE_VALID = 1u;
public static const uint DDGI_PROBE_STATE_RELOCATION_FAILED = 2u;

public struct DdgiProbeMetadata
{
    public float4 nominal_position_and_min_clearance;
    public float4 actual_position_and_clearance;
    public uint4 state_and_reserved;
};

public struct B_DdgiProbeMetadata
{
    public DdgiProbeMetadata data[];
};

public struct B_DdgiRayData
{
    public float4 data[];
};

public struct B_DdgiTraceStats
{
    public uint data[];
};
