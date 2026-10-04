import flower_model;
import flower_stem_geometry;
import flower_stem_experiment;
import flower_stem_model_sampling;
import stem_proxy_projection;
struct VertexInput {
    [[vk::location(0)]] uint vertex:TEXCOORD0;
    uint instance:SV_VulkanInstanceID;
};
[shader("vertex")]
StemProxyVertex main(VertexInput input) {
    FlowerPlantPose pose=flowerPlantPose(input.instance);
    StemProxyVertex output;
    output.position=float4(2,2,2,1);output.ray=float3(0,0,1);
    output.root_scale=float4(pose.frame.center-pose.tip_offset-pose.attachment_offset,pose.frame.scale);
    output.yaw_height_bend=float4(pose.frame.axisX.x,pose.frame.axisX.z,pose.height,pose.bend);
    output.motion_edge=float4(0,0,0,flowerPart(0u).distribution.y);
    output.socket=flowerPart(1u).socket;
    output.grid_center_cell=float4(0);output.grid_x=float3(1,0,0);output.grid_y=float3(0,1,0);
    if(pose.frame.scale<=1e-7)return output;
    output.motion_edge.xyz=stemToLocalVector(output,pose.tip_offset)/pose.frame.scale;
    StemGeometry s=stemGeometry(output);
    output.socket.w=dot(output.socket.xyz,stemCenter(s,0u,1.0));
    float3 lo,hi;stemBounds(s,lo,hi);
    StemProjection projection=projectStemProxy(input.vertex,camera_info,
        output.root_scale.xyz,pose.frame.scale,output.yaw_height_bend.xy,
        lo,hi,float3(s.bend*0.5,s.height*0.5,0),s.height,
        flowerStemSamplingSettings().z,flowerStemSamplingSettings().y==1u);
    output.position=projection.position;output.ray=projection.ray;
    output.grid_center_cell=float4(projection.grid.center,projection.grid.cell);
    output.grid_x=projection.grid.x;output.grid_y=projection.grid.y;
    return output;
}
