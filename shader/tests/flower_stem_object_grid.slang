import flower_stem_geometry;
import model_pixel_projection;
import tracer_types;
export __extern_cpp int main() {
    StemGeometry s;
    s.height=2.1;s.bend=.08;s.radius=.025;s.motion=float3(0);s.branches=1u;
    s.socket=float4(0,1,0,s.height);
    float3 center;float radius;stemObjectFrame(s,center,radius);
    StemGeometry windy=s;windy.motion=float3(.2,0,-.15);
    float3 c2;float r2;stemObjectFrame(windy,c2,r2);
    if(any(center!=c2)||radius!=r2)return 1; // no wind-driven frame breathing
    for(uint branch=0u;branch<3u;branch++)for(uint i=0u;i<=24u;i++) {
        float t=float(i)/24.0;
        if(length(stemCenter(windy,branch,t)-center)+stemRadius(windy,branch,t)>radius)return 2;
    }
    ModelPixelView view=modelPixelView(float4(1,0,0,1),float4(0,1,0,0),float4(0,0,1,0));
    uint nearAngular=0u,farAngular=0u;
    for(uint zoom=0u;zoom<3u;zoom++) {
        float distance=zoom==0u?4.0:zoom==1u?16.0:64.0;
        float3 cameraOrigin=center+float3(0,0,distance);
        uint angularCells=0u;float last=-1e20;
        for(uint i=0u;i<512u;i++) {
            float3 target=center+float3(0,(float(i)/511.0-.5)*s.height,0);
            float3 ray=stemDirectionCell(normalize(target-cameraOrigin),550u);
            float key=ray.y/abs(ray.z);
            if(key!=last)angularCells++;last=key;
        }
        if(zoom==0u)nearAngular=angularCells;if(zoom==2u)farAngular=angularCells;
    }
    if(nearAngular<=farAngular*4u)return 3; // captures A's distance-dependent loss
    for(uint size=0u;size<3u;size++) {
        uint n=size==0u?32u:size==1u?128u:256u;
        uint firstHits=0u;float firstDepth=0.0;
        for(uint zoom=0u;zoom<3u;zoom++) {
            float distance=zoom==0u?4.0:zoom==1u?16.0:64.0;
            U_CameraInfo camera=(U_CameraInfo)0;
            camera.pos=float4(center+float3(0,0,distance),1);
            camera.view_mat=float4x4(1,0,0,-center.x, 0,1,0,-center.y, 0,0,1,-center.z-distance, 0,0,0,1);
            camera.proj_mat=float4x4(1,0,0,0, 0,1,0,0, 0,0,-1,-.1, 0,0,-1,0);
            camera.view_proj_mat=mul(camera.proj_mat,camera.view_mat);
            float2 lo=modelOrthographicQuad(view,center,radius,float2(0),camera).xy;
            float2 hi=modelOrthographicQuad(view,center,radius,float2(1),camera).xy;
            if(abs((hi.x-lo.x)*distance-2.0*radius)>1e-5)return 4;
            uint hits=0u;float depths=0;
            for(uint y=0u;y<n;y++)for(uint x=0u;x<n;x++) {
                float2 uv=(float2(x,y)+.5)/float(n);
                float2 screen=modelOrthographicQuad(view,center,radius,uv,camera).xy;
                float2 cell=stemObjectCell((screen-lo)/(hi-lo),n);
                if(any(abs(cell-uv)>1e-6))return 5;
                float3 origin=stemObjectRayOrigin(center,radius,view.x,view.y,view.z,cell);
                StemHit hit=traceStem(s,origin,-view.z);
                if(hit.distance<1e19) {
                    hits++;depths+=hit.distance;
                    if(!stemInsideSocket(s,hit.point))return 6;
                    float depth=modelOrthographicDepth((radius-dot(hit.point-center,view.z))/(2.0*radius),center,radius,camera);
                    if(depth<=0.0||depth>=1.0||isnan(depth))return 7;
                }
            }
            if(hits==0u)return 8;
            if(zoom==0u){firstHits=hits;firstDepth=depths;}
            else if(hits!=firstHits||depths!=firstDepth)return 9; // same source mask/data, not just same setting
        }
    }
    return 0;
}
