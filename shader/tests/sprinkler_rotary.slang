import sprinkler_animation;
import sprinkler_mist_transport;
import terrain_wetting;

export __extern_cpp int main()
{
    uint failures=0;
    float sum=0.0;
    for(uint n=0u;n<4u;++n)sum+=mistRotaryEmission(n);
    if(abs(sum-2.0)>1e-6)failures++;
    for(uint tick=0u;tick<80u;++tick) {
        float a=sprinklerRotaryAngle(tick,0.5,0.05,0.125);
        if(length(sprinklerRotate(float3(1,0,0),a)-sprinklerRotaryDirectionAtTick(float(tick)+0.5,0.05,0.125,0u))<0.1)failures++; // nozzle yaw is not the arm's radial axis
        for(uint n=0u;n<3u;++n) {
            float3 p=sprinklerRotaryOutletAtTick(float(tick)+0.5,0.05,0.125,n);
            float3 d=sprinklerRotaryDirectionAtTick(float(tick)+0.5,0.05,0.125,n);
            float3 older=sprinklerRotaryDirectionAtTick(float(tick)+0.5-20.0,0.05,0.125,n);
            if(abs(p.y-4.2/256.0)>1e-7 || abs(length(d)-1.0)>1e-6 || abs(dot(d,older))>1e-5)failures++;
            float3 oldR=mistFlowRadii(0.125,0.25,n),r=mistFlowRadii(0.125,0.25,n,true);
            if(abs(r.x-r.z)>1e-7 || abs(r.x*r.y*r.z-oldR.x*oldR.y*oldR.z)>1e-9)failures++;
            float3 next=sprinklerRotaryDirectionAtTick(float(tick)+0.5,0.05,0.125,(n+1u)%3u);
            if(abs(dot(d,next)+0.5)>1e-5)failures++;
        }
    }
    uint aperture=0u;
    for(uint i=0u;i<8192u;++i) {
        float angle=(float(i)+0.5)*6.28318530718/8192.0;
        aperture+=wetPatchCenterInRotarySector(float2(cos(angle),sin(angle)),float2(1,0))?1u:0u;
    }
    if(aperture!=4096u)failures++;
    for(uint size=0u;size<4u;++size) {
        float scales[4]={2.0,4.0,12.0,32.0};
        float scale=scales[size];
        uint4 histogram=uint4(0);
        float outside=0.0;
        for(uint i=0u;i<32768u;++i) {
            uint seed=i*1664525u+1013904223u;
            float2 origin=float2(196.5,360.5);
            float angle=float(i%1024u)*6.28318530718/1024.0;
            float2 axis=float2(cos(angle),sin(angle));
            float2 complement=float2(0.5*axis.x-0.866025403784*axis.y,0.866025403784*axis.x+0.5*axis.y);
            float3 point=float3(origin.x+31.75,127.5,origin.y+17.25);
            WetPatchSample a=wetPatchDirectionalSample(point,scale,seed,origin,axis,true);
            WetPatchSample b=wetPatchDirectionalSample(point,scale,seed,origin,complement,true);
            if(abs(a.coverage+b.coverage-1.0)>1e-5)failures++;
            if(a.coverage>0.0 && !wetPatchCanReachRotarySector(point.xz-origin,axis,scale))failures++;
            uint wet=min(3u,wetPatchPoissonIncrement(0.8*a.coverage,a.uniform)+wetPatchPoissonIncrement(0.8*b.coverage,b.uniform));
            histogram[wet]++;
            float phi=31.0*3.14159265359/180.0;
            WetPatchSample edge=wetPatchDirectionalSample(float3(origin.x+20.0*cos(phi),127.5,origin.y+20.0*sin(phi)),scale,seed,origin,float2(1,0),true);
            outside+=float(wetPatchPoissonIncrement(0.8*edge.coverage,edge.uniform));
        }
        float4 expected=float4(exp(-0.8),0.8*exp(-0.8),0.32*exp(-0.8),1.0-2.12*exp(-0.8));
        if(any(abs(float4(histogram)/32768.0-expected)>0.014) || outside<=0.0)failures++;
        printf("rotary scale %.0f distribution %u/%u/%u/%u outside %.0f\n",scale,histogram.x,histogram.y,histogram.z,histogram.w,outside);
    }
    printf("rotary contracts failures=%u\n",failures);
    return int(failures!=0u);
}
