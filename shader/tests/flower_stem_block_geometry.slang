import flower_stem_geometry;
import flower_stem_blocks;
// Slow exhaustive interval-union oracle, deliberately not the row-ordered
// production algorithm. Includes cameras inside solids and horizontal ties.
float referenceDistance(StemGeometry s,float3 ro,float3 rd,float cell) {
    uint rows=uint(ceil(s.height/cell));float first=1e20,exit=-1;
    for(uint row=0u;row<rows;row++)for(uint b=0u;b<(s.branches!=0u?3u:1u);b++) {
        StemBlockInterval i=stemBlockInterval(s,ro,rd,row,b,cell);
        if(i.hi<i.lo||i.hi<=1e-5)continue;
        if(i.lo<=1e-5)exit=max(exit,i.hi);else first=min(first,i.lo);
    }
    if(exit<0)return first;
    for(uint pass=0u;pass<rows*3u;pass++) {
        float old=exit;
        for(uint row=0u;row<rows;row++)for(uint b=0u;b<(s.branches!=0u?3u:1u);b++) {
            StemBlockInterval i=stemBlockInterval(s,ro,rd,row,b,cell);
            if(i.hi>=i.lo&&i.lo<=exit+1e-5)exit=max(exit,i.hi);
        }
        if(exit==old)return exit;
    }
    return -1;
}
export __extern_cpp int main() {
    StemGeometry s;s.height=2.1;s.bend=.08;s.radius=.025;
    s.motion=float3(0);s.branches=1u;s.socket=float4(0,1,0,s.height);
    uint fineHits=0u,coarseHits=0u;
    for(uint variant=0u;variant<3u;variant++) {
        float cell=variant==0u?.0125:variant==1u?.05:.2;
        for(uint wind=0u;wind<2u;wind++) {
            s.motion=wind==0u?float3(0):float3(.25,-.15,.13);
            s.socket=float4(normalize(float3(0,1,.4)),0);
            s.socket.w=dot(s.socket.xyz,stemCenter(s,0u,1.0));
            for(uint test=0u;test<90u;test++) {
                float3 ro=float3((float(test%9u)-4.0)*.035,float(test/9u)*.21,1.0);
                float3 rd=normalize(float3(.03,-.13,-1));
                if(test<30u)rd=float3(0,0,-1); // horizontal including shared faces
                if(test>=60u){ro=stemCenter(s,0u,float(test%10u)/10.0);rd=normalize(float3(.1,test%2u==0u?1.0:-1.0,.07));}
                StemHit hit=traceStemBlocks(s,ro,rd,cell);
                float expected=referenceDistance(s,ro,rd,cell);
                if(abs(hit.distance-expected)>1e-4)return 1;
                if(hit.distance<1e19) {
                    if(any(isnan(hit.point))||any(isnan(hit.normal))||abs(length(hit.normal)-1)>1e-4)return 2;
                    if(!stemInsideSocket(s,hit.point))return 3;
                    if(wind==0u&&variant==0u)fineHits++;
                    if(wind==0u&&variant==2u)coarseHits++;
                }
            }
        }
    }
    if(fineHits==0u||coarseHits==0u||fineHits==coarseHits)return 4;
    // Inside-camera exit must be the outer top, not an internal Y slab face.
    s.motion=float3(0);s.branches=0u;s.socket=float4(0,1,0,s.height);
    s.bend=0;
    StemHit up=traceStemBlocks(s,float3(0,.4,0),float3(0,1,0),.05);
    if(abs(up.distance-1.7)>1e-5||up.normal.y<.99)return 5;
    StemHit down=traceStemBlocks(s,float3(0,.4,0),float3(0,-1,0),.05);
    if(abs(down.distance-.4)>1e-5||down.normal.y>-.99)return 6;
    // Tiny height / thick radius and a tilted cap remain finite and clipped.
    s.height=.013;s.radius=.08;s.socket=float4(normalize(float3(0,1,.7)),0);
    s.socket.w=dot(s.socket.xyz,stemCenter(s,0u,1.0));
    StemHit cap=traceStemBlocks(s,float3(0,1,0),float3(0,-1,0),.2);
    if(cap.distance>1e19||!stemInsideSocket(s,cap.point)||any(isnan(cap.normal)))return 7;
    return 0;
}
