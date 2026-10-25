module vegetation_water_types;
public struct WaterResponseState {
    public float4 displacement_wetness; // local voxel displacement, one surface-wetness value
    public float4 velocity; // local voxels/second; w reserved
};
public struct WaterResponseInput {
    public uint4 key; // world voxel receiver, species
    public uint4 history; // previous dense state, seed, attachment anchor packed xyz? see anchor
    public uint4 anchor; // rest attachment anchor xyz; w reserved
};
public struct VegetationWaterLobe {
    public float4 center_exposure; // transport emission; never optical density
    public float4 radius;
    public float4 velocity;
    public float4 lower;
    public float4 upper;
};
public struct WaterResponseStep {
    public float dt;
    public uint count;
    public uint sources;
    public uint lobes;
    public uint reset;
    public uint remap;
    public uint enabled;
    public uint reserved;
};
public uint waterKeyHash(uint4 key) {
    uint h=key.x*73856093u^key.y*19349663u^key.z*83492791u^key.w*2654435761u;
    h^=h>>16;h*=0x7feb352du;h^=h>>15;return h;
}
