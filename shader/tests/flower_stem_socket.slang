// Actual production trace/socket/display math: no stem on the flower's outward
// side, including a direction texel whose plane footprint crosses the socket.
import flower_stem_geometry;
export __extern_cpp int main() {
    float tilts[6]={38,28,42,44,55,70};
    for(uint species=0u;species<6u;species++)for(uint variant=0u;variant<4u;variant++) {
        StemGeometry s;
        s.height=variant==3u?0.01:2.1;s.bend=0.08;
        s.radius=variant==0u?.025*1.33:variant==1u?.00625:.1;
        s.motion=float3(.12,0,-.09);s.branches=variant%2u;
        float3 tip=stemCenter(s,0u,1.0);
        float angle=tilts[species]*3.14159265359/180.0;
        float3 normal=float3(0,sin(angle),cos(angle));
        s.socket=float4(normal,dot(normal,tip));
        // The old source returned a rounded sphere cap above the head here.
        StemHit top=traceStem(s,tip+normal, -normal);
        if(top.distance>=1e19||!stemInsideSocket(s,top.point))return 1;
        if(abs(dot(normal,top.point-tip))>2e-5)return 2;
        if(dot(top.normal,normal)<.99)return 3;
        // The socket is closed, including origins inside the terminal hull.
        StemHit inside=traceStem(s,tip-normal*.001,normal);
        if(inside.distance>=1e19||abs(inside.distance-.001)>2e-5)return 4;
        // Clip every side/branch candidate, not just the first rounded cap.
        for(uint view=0u;view<24u;view++) {
            float a=float(view)*6.28318530718/24.0;
            float3 ro=tip+float3(cos(a),.3,sin(a));
            float3 displayRay=normalize(tip-normal*.015-ro);
            for(uint n=128u;n<=2048u;n*=2u) {
                float3 ray=stemDirectionCell(displayRay,n);
                StemHit hit=traceStem(s,ro,ray);
                if(hit.distance<1e19&&!stemInsideSocket(s,hit.point))return 5;
            }
        }
        // A source cap sample alone is insufficient: display rays offset from
        // it reach the outward half-space. Production uses this predicate on
        // the remapped actual displayed point, rejecting rather than biasing Z.
        float3 ro=tip+float3(0,0,1),sourceRay=float3(0,0,-1);
        float3 displayRay=normalize(tip+normal*.002-ro);
        float3 remapped=stemDirectionDisplayPoint(ro,displayRay,sourceRay,tip);
        if(stemInsideSocket(s,remapped))return 6;
    }
    return 0;
}
