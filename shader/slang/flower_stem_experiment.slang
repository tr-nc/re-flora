// Flower path adapter. No screen-space proxy, sample rays or camera state.
module flower_stem_experiment;
import flower_model;
import flower_stem_geometry;

public struct FlowerStemFrame {
    public float4 root_scale;
    // Plant yaw (axisX.x/z), height and rest bend, all camera-independent.
    public float4 yaw_height_bend;
    public float4 motion_edge;
    public float4 socket;
};
public float3 stemToWorldVector(FlowerStemFrame p,float3 v) {
    float c=p.yaw_height_bend.x,s=p.yaw_height_bend.y;
    return float3(c*v.x-s*v.z,v.y,s*v.x+c*v.z);
}
public float3 stemToLocalVector(FlowerStemFrame p,float3 v) {
    float c=p.yaw_height_bend.x,s=p.yaw_height_bend.y;
    return float3(c*v.x+s*v.z,v.y,-s*v.x+c*v.z);
}
public StemGeometry stemGeometry(FlowerStemFrame p) {
    StemGeometry s;
    s.height=p.yaw_height_bend.z;s.bend=p.yaw_height_bend.w;
    s.radius=p.motion_edge.w*0.5*flowerStemShapeSettings().x;
    s.tipRadiusRatio=flowerStemShapeSettings().w;
    s.motion=p.motion_edge.xyz;s.branches=flowerStemSamplingSettings().w;s.socket=p.socket;
    return s;
}
public float3 stemWorldPoint(FlowerStemFrame p,float3 local) {
    return p.root_scale.xyz+stemToWorldVector(p,local)*p.root_scale.w;
}
