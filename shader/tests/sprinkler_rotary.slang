import sprinkler_animation;
import sprinkler_mist_transport;
import terrain_wetting;

export __extern_cpp int main()
{
    uint failures=0;
    float sum=0.0;
    for(uint n=0u;n<4u;++n) {
        sum+=mistRotaryEmission(n);
        if(n>0u && mistRotaryEmission(n)!=0.0)failures++;
    }
    if(abs(sum-2.0)>1e-6)failures++;
    for(uint tick=0u;tick<80u;++tick) {
        float a=sprinklerRotaryAngle(tick,0.5,0.05,0.125);
        float3 p=sprinklerRotaryOutletAtTick(float(tick)+0.5,0.05,0.125);
        float3 d=sprinklerRotaryDirectionAtTick(float(tick)+0.5,0.05,0.125);
        float3 older=sprinklerRotaryDirectionAtTick(float(tick)+0.5-20.0,0.05,0.125);
        if(length(sprinklerRotate(float3(1,0,0),a)-d)<0.1)failures++; // mouth yaw is not the arm's radial axis
        if(abs(p.y-4.2/256.0)>1e-7 || abs(length(d)-1.0)>1e-6 || abs(dot(d,older))>1e-5)failures++;
    }
    uint aperture=0u;
    for(uint i=0u;i<12288u;++i) {
        float angle=(float(i)+0.5)*6.28318530718/12288.0;
        aperture+=wetPatchCenterInRotarySector(float2(cos(angle),sin(angle)),float2(1,0))?1u:0u;
    }
    if(aperture!=2048u || abs(float(aperture)*WET_ROTARY_DOSE_SCALE/12288.0-0.5)>1e-6)failures++;
    // No hidden outlets at 120/240 degrees, nor backwards watering.
    if(wetPatchCenterInRotarySector(float2(-0.5,0.8660254),float2(1,0)) ||
       wetPatchCenterInRotarySector(float2(-0.5,-0.8660254),float2(1,0)) ||
       wetPatchCenterInRotarySector(float2(-1,0),float2(1,0)))failures++;
    for(uint size=0u;size<4u;++size) {
        float scales[4]={2.0,4.0,12.0,32.0};
        float scale=scales[size];
        uint4 histogram=uint4(0);
        float outside=0.0;
        for(uint i=0u;i<16384u;++i) {
            uint seed=i*1664525u+1013904223u;
            float2 origin=float2(196.5,360.5);
            float angle=float(i%1024u)*6.28318530718/1024.0;
            float3 point=float3(origin.x+31.75,127.5,origin.y+17.25);
            float coverage=0.0;
            // Six successive single-nozzle directions cover one revolution.
            for(uint sector=0u;sector<6u;++sector) {
                float a=angle+float(sector)*6.28318530718/6.0;
                float2 axis=float2(cos(a),sin(a));
                WetPatchSample sample=wetPatchDirectionalSample(point,scale,seed,origin,axis,true);
                coverage+=sample.coverage;
                if(sample.coverage>0.0 && !wetPatchCanReachRotarySector(point.xz-origin,axis,scale))failures++;
            }
            if(abs(coverage-1.0)>1e-5)failures++;
            // For the distribution check use a fixed unjittered-in-time lattice,
            // so sector coverage sums exactly to one for each trial.
            uint wet=0u;
            for(uint sector=0u;sector<6u;++sector) {
                float a=angle+float(sector)*6.28318530718/6.0;
                WetPatchSample sample=wetPatchDirectionalSample(point,scale,seed,origin,float2(cos(a),sin(a)),true);
                wet+=wetPatchPoissonIncrement((0.8/3.0)*WET_ROTARY_DOSE_SCALE*sample.coverage,sample.uniform);
            }
            histogram[min(3u,wet)]++;
            float phi=31.0*3.14159265359/180.0;
            WetPatchSample edge=wetPatchDirectionalSample(float3(origin.x+20.0*cos(phi),127.5,origin.y+20.0*sin(phi)),scale,seed,origin,float2(1,0),true);
            outside+=float(wetPatchPoissonIncrement(0.8*edge.coverage,edge.uniform));
            if(wetPatchDirectionalSample(float3(origin.x-80.0,127.5,origin.y),scale,seed,origin,float2(1,0),true).coverage>0.0)failures++;
        }
        float4 expected=float4(exp(-0.8),0.8*exp(-0.8),0.32*exp(-0.8),1.0-2.12*exp(-0.8));
        if(any(abs(float4(histogram)/16384.0-expected)>0.02) || outside<=0.0)failures++;
        printf("single-nozzle scale %.0f distribution %u/%u/%u/%u outside %.0f\n",scale,histogram.x,histogram.y,histogram.z,histogram.w,outside);
    }
    printf("rotary contracts failures=%u\n",failures);
    return int(failures!=0u);
}
