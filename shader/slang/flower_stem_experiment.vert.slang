import flower_model;
import flower_stem_geometry;
import flower_stem_experiment;
import model_pixel_cache;
import model_pixel_views;
import model_pixel_projection;
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
    output.object_x=output.object_y=output.object_z=output.object_center=float4(0);output.object_uv=float2(0);
    if(pose.frame.scale<=1e-7)return output;
    output.motion_edge.xyz=stemToLocalVector(output,pose.tip_offset)/pose.frame.scale;
    StemGeometry s=stemGeometry(output);
    output.socket.w=dot(output.socket.xyz,stemCenter(s,0u,1.0));
    uint corners[6]={0u,1u,2u,0u,2u,3u};
    uint i=corners[input.vertex];
    float2 uv=float2(i==1u||i==2u?1.0:0.0,i>=2u?1.0:0.0),ndc;
    if(flowerStemShapeSettings().w>0.0) {
        float3 center;float radius;stemObjectFrame(s,center,radius);
        float3 pivot=stemWorldPoint(output,center);radius*=pose.frame.scale;
        uint views=modelSourceViewCount(flowerPart(1u).range.z);
        float3 delta=camera_info.pos.xyz-pivot;
        uint key=nearestModelView(length(delta)>1e-8?normalize(delta):float3(0,0,1),views);
        // Keep canonical directions in WORLD space, matching A's rotation-stable
        // orientation. Source origins are object anchored, unlike angular A.
        ModelPixelView view=canonicalModelView(key,views);
        view.roll=modelObjectRoll(view.x,view.z,pivot,camera_info);
        output.object_x=float4(view.x,view.roll.x);output.object_y=float4(view.y,view.roll.y);
        output.object_z=float4(view.z,float(key));output.object_center=float4(pivot,radius);
        output.object_uv=uv;
        output.position=modelOrthographicQuad(view,pivot,radius,uv,camera_info);
        ndc=output.position.xy;
    } else {
        // Unchanged A/continuous/surface conservative analytic proxy.
        float3 lo,hi;stemBounds(s,lo,hi);
        if(flowerStemSamplingSettings().y==1u) {
            float distance=length(camera_info.pos.xyz-output.root_scale.xyz)+
                max(length(lo),length(hi))*pose.frame.scale;
            float margin=distance*4.0/float(flowerStemSamplingSettings().z)/pose.frame.scale;
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
