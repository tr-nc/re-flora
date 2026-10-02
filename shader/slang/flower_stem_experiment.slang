module flower_stem_experiment;
import flower_model;
import flower_stem_geometry;
import tracer_types;
[[vk::binding(3,0)]] public ConstantBuffer<U_CameraInfo> camera_info;

public struct StemProxyVertex {
    public float4 position:SV_Position;
    [[vk::location(0)]] public noperspective float3 ray:TEXCOORD0;
    [[vk::location(1)]] public nointerpolation float4 root_scale:TEXCOORD1;
    // Plant yaw (axisX.x/z), height and rest bend, all camera-independent.
    [[vk::location(2)]] public nointerpolation float4 yaw_height_bend:TEXCOORD2;
    [[vk::location(3)]] public nointerpolation float4 motion_edge:TEXCOORD3;
    [[vk::location(4)]] public nointerpolation float4 socket:TEXCOORD4;
};
public float3 stemToWorldVector(StemProxyVertex p,float3 v) {
    float c=p.yaw_height_bend.x,s=p.yaw_height_bend.y;
    return float3(c*v.x-s*v.z,v.y,s*v.x+c*v.z);
}
public float3 stemToLocalVector(StemProxyVertex p,float3 v) {
    float c=p.yaw_height_bend.x,s=p.yaw_height_bend.y;
    return float3(c*v.x+s*v.z,v.y,-s*v.x+c*v.z);
}
public StemGeometry stemGeometry(StemProxyVertex p) {
    StemGeometry s;
    s.height=p.yaw_height_bend.z;s.bend=p.yaw_height_bend.w;
    s.radius=p.motion_edge.w*0.5*flowerStemShapeSettings().x;
    s.motion=p.motion_edge.xyz;s.branches=flowerStemSamplingSettings().w;s.socket=p.socket;
    return s;
}
public float3 stemWorldPoint(StemProxyVertex p,float3 local) {
    return p.root_scale.xyz+stemToWorldVector(p,local)*p.root_scale.w;
}
