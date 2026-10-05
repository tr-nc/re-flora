import stem_band_geometry;
bool close(float3 a,float3 b) {return all(abs(a-b)<2e-5);}
export __extern_cpp int main() {
    float3 a=float3(0),b=float3(0,2,0),up=float3(0,1,0),side=float3(1,0,0);
    for(uint corner=0u;corner<8u;corner++) {
        float3 q=stemBandCorner(corner);
        float3 p=stemBandPoint(a,b,up,up,side,side,float2(0.5),float2(0.5),q);
        if(!close(p,float3(q.x,2*(q.y+0.5),q.z)))return 1;
    }
    // Shared boundary frame/radius yields identical rings across bent bands.
    float3 bend=normalize(float3(0.7,1,0.3)),joint=float3(0.3,2,0.1);
    for(uint corner=0u;corner<8u;corner++) {
        float3 q=stemBandCorner(corner);q.y=0.5;
        float3 first=stemBandPoint(a,joint,up,bend,side,side,float2(0.5),float2(0.3),q);
        q.y=-0.5;
        float3 second=stemBandPoint(joint,float3(1,3,0.6),bend,bend,side,side,float2(0.3),float2(0.2),q);
        if(!close(first,second))return 2;
    }
    for(uint i=0u;i<32u;i++) {
        float3 axis=normalize(float3(float(int(i)-16),1,float(i%5u)));
        float3 p=stemBandPoint(a,axis,axis,axis,axis,axis,float2(0.5),float2(0.1),float3(0.5));
        if(any(isnan(p))||any(isinf(p)))return 3;
    }
    // Both cached and direct grass use these same shared midpoint frames.
    float3 pa,pb,pta,ptb,na,nb,nta,ntb;
    float3 centers[3] = { float3(0), float3(0.4,1,0.1), float3(0.7,2,-0.2) };
    stemBandCenterFrames(centers[0],centers[0],centers[1],false,true,0.5,pa,pb,pta,ptb);
    stemBandCenterFrames(centers[1],centers[0],centers[2],true,true,0.5,na,nb,nta,ntb);
    if(!close(pb,na)||!close(ptb,nta)||!close(pa,float3(0,-0.5,0)))return 7;
    stemBandCenterFrames(centers[2],centers[1],centers[2],true,false,0.5,pa,pb,pta,ptb);
    if(!close(nb,pa)||!close(ntb,pta)||!close(pb,centers[2]+ptb*0.5))return 8;
    stemBandCenterFrames(centers[0],centers[0],centers[0],true,true,0.5,pa,pb,pta,ptb);
    if(any(isnan(pta))||any(isnan(ptb))||!close(pta,float3(0,1,0)))return 9;
    if(stemColorBand(0,10,8)!=0u||stemColorBand(10,10,8)!=7u)return 4;
    if(stemColorBand(2,10,8)!=1u||stemColorBand(2.5,10,8)!=2u)return 5;
    return 0;
}
