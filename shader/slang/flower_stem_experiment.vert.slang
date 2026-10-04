import flower_model;
import flower_stem_geometry;
import flower_stem_experiment;
import flower_stem_model_sampling;
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
    if(flowerStemShapeSettings().y!=0.0)
        output.motion_edge.w=stemWorldCellSize(flowerStemShapeSettings().z,pose.frame.scale);
    output.motion_edge.xyz=stemToLocalVector(output,pose.tip_offset)/pose.frame.scale;
    StemGeometry s=stemGeometry(output);
    output.socket.w=dot(output.socket.xyz,stemCenter(s,0u,1.0));
    uint corners[6]={0u,1u,2u,0u,2u,3u};
    uint i=corners[input.vertex];
    float2 uv=float2(i==1u||i==2u?1.0:0.0,i>=2u?1.0:0.0),ndc;
    {
        // Conservative analytic proxy shared by both independent effects.
        float3 lo,hi;stemBounds(s,lo,hi);
        if(flowerStemSamplingSettings().y==1u) {
            float3 center=float3(s.bend*0.5,s.height*0.5,0);
            float radius=length(max(abs(lo-center),abs(hi-center)));
            uint resolution=flowerStemSamplingSettings().z;
            float3 right=mul(camera_info.view_mat_inv,float4(1,0,0,0)).xyz;
            StemModelGrid grid=stemModelGrid(stemWorldPoint(output,center),radius*pose.frame.scale,
                s.height*pose.frame.scale,resolution,camera_info.pos.xyz,right);
            output.grid_center_cell=float4(grid.center,grid.cell);
            output.grid_x=grid.x;output.grid_y=grid.y;
            // Conservative perspective fringe, including inside-bound refinement.
            float margin=2.0*s.height/float(resolution);
            lo-=margin;hi+=margin;
        }
        float2 ndcLo=float2(1e20),ndcHi=float2(-1e20);
        bool crossesNear=false,anyInFront=false;
        for(uint corner=0u;corner<8u;corner++) {
            float3 local=float3((corner&1u)!=0u?hi.x:lo.x,(corner&2u)!=0u?hi.y:lo.y,(corner&4u)!=0u?hi.z:lo.z);
            float4 clip=mul(camera_info.view_proj_mat,float4(stemWorldPoint(output,local),1));
            anyInFront=anyInFront||clip.w>0.0;
            crossesNear=crossesNear||clip.z<=0.0||clip.w<=1e-7;
            if(clip.w>1e-7) {ndcLo=min(ndcLo,clip.xy/clip.w);ndcHi=max(ndcHi,clip.xy/clip.w);}
        }
        if(!anyInFront)return output;
        if(crossesNear) {ndcLo=float2(-1);ndcHi=float2(1);}
        ndcLo=clamp(ndcLo,float2(-1),float2(1));ndcHi=clamp(ndcHi,float2(-1),float2(1));
        ndc=lerp(ndcLo,ndcHi,uv);output.position=float4(ndc,0,1);
    }
    float4 viewRay=mul(camera_info.proj_mat_inv,float4(ndc,1,1));
    output.ray=mul(camera_info.view_mat_inv,float4(viewRay.xyz,0)).xyz;
    return output;
}
