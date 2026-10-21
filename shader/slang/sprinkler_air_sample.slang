module sprinkler_air_sample;
import sprinkler_air_math;

[[vk::binding(0,2)]] StructuredBuffer<B_SprinklerAirInfo> sprinkler_air_info;
[[vk::binding(1,2)]] StructuredBuffer<uint> sprinkler_air_density;

public B_SprinklerAirInfo airCurrentInfo() { return sprinkler_air_info[0]; }

float airLoad(int3 cell,uint side)
{
    if(any(cell<int3(0)) || any(cell>=int3(side)))return 0.0;
    return float(sprinkler_air_density[airCellIndex(uint3(cell),side)])/SPRINKLER_AIR_FIXED_SCALE;
}

public float airSampleDensity(float3 world,B_SprinklerAirInfo info)
{
    float3 p=(world-info.lower_valid.xyz)/info.cell_side.xyz-0.5;
    int3 cell=int3(floor(p));
    float3 f=frac(p);
    uint side=uint(info.cell_side.w);
    float z0=lerp(lerp(airLoad(cell,side),airLoad(cell+int3(1,0,0),side),f.x),
                  lerp(airLoad(cell+int3(0,1,0),side),airLoad(cell+int3(1,1,0),side),f.x),f.y);
    float z1=lerp(lerp(airLoad(cell+int3(0,0,1),side),airLoad(cell+int3(1,0,1),side),f.x),
                  lerp(airLoad(cell+int3(0,1,1),side),airLoad(cell+int3(1,1,1),side),f.x),f.y);
    return lerp(z0,z1,f.z);
}
