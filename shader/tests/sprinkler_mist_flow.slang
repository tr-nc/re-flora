import sprinkler_mist_math;
import sprinkler_mist_transport;

float kernel(float3 p,float3 center,float3 radii) {
    float3 q=(p-center)/radii;
    float e=max(0.0,1.0-dot(q,q));return e*e;
}

float freeDensity(float3 point,float tick) {
    float density=0;
    for(uint nozzle=0;nozzle<4;++nozzle)for(uint n=0;n<12;++n) {
        float age=(float(n)+0.5)*0.25;
        float3 p,v;
        float3 direction=mistFlowDirection(nozzle);
        mistFlowAdvance(direction*(3.1/256.0)+float3(0,4.5/256.0,0),
            direction*0.43+float3(0,0.22,0),float3(0,-MIST_FLOW_SETTLING_SPEED,0),age,p,v);
        float3 radii=mistFlowRadii(age,0.25,nozzle);
        float emission=mistPairEmission(tick-age/0.05,5.0,0.05,0.0,nozzle);
        density+=kernel(point,p,radii)*mistKernelAmplitude(mistVisibleAmount(age,0.25,emission),radii);
    }
    return density;
}

export __extern_cpp int main()
{
    // Normalization is independent of spatial scale; widening must not create mist.
    float sum=0;
    for(uint i=0;i<4096;++i) {
        float r=(float(i)+0.5)/4096.0;
        sum+=12.5663706*r*r*(1-r*r)*(1-r*r)/4096.0;
    }
    float3 small=float3(0.03,0.04,0.05),large=small*2.0;
    if(abs(sum*mistKernelAmplitude(1.0,float3(1))-1.0)>0.00001)return 1;
    if(abs(mistKernelAmplitude(1,small)/mistKernelAmplitude(1,large)-8.0)>0.00001)return 2;
    if(mistVisibleAmount(2,0.25,1)>=mistVisibleAmount(1,0.25,1))return 3;

    // Exact transfer is stable for thin, oblique, clipped and camera-inside paths.
    float3 center=float3(0.2,-0.1,0.3),radii=float3(0.14,0.05,0.09);
    for(uint test=0;test<40;++test) {
        float3 origin=center+float3(0.003*float(test%20u),0,test<20u ? -0.3 : -10.0);
        float3 direction=normalize(float3(-0.05,0.002*float(test),1));
        float2 interval;
        if(test>=20u)direction=normalize(center-origin);
        if(!mistRayInterval(origin,direction,center,radii,test<20u ? 0.31+float(test)*0.01 : 20.0,interval))continue;
        float numeric=0,dt=(interval.y-interval.x)/2048.0;
        for(uint s=0;s<2048;++s)numeric+=kernel(origin+direction*(interval.x+(float(s)+0.5)*dt),center,radii)*dt;
        float exact=mistKernelLineIntegral(origin,direction,center,radii,interval);
        if(abs(numeric-exact)>0.00001 || !isfinite(exact))return 4;
    }
    float inside=mistKernelLineIntegral(float3(0),float3(1,0,0),float3(0),float3(1),float2(0,1));
    if(abs(inside-8.0/15.0)>0.00001)return 5;

    // Damped momentum has a consistent time step and eventually settles below
    // source height without a sprinkler-base clipping plane or unbounded gravity.
    float3 p,v,p1,v1,p2,v2;
    float3 start=float3(0,0.02,0),velocity=float3(0.43,0.22,0),target=float3(0.02,-0.14,0.01);
    mistFlowAdvance(start,velocity,target,3,p,v);
    mistFlowAdvance(start,velocity,target,1.0,p1,v1);
    mistFlowAdvance(p1,v1,target,2.0,p2,v2);
    if(length(p-p2)>0.000001 || length(v-v2)>0.000001)return 6;
    if(p.y>=-0.1 || p.x<=0.2 || abs(v.y)>0.15)return 7;

    // Both nozzle pairs share one bounded emission amount, including negative
    // birth times / phase wrap; old emission survives the instantaneous swap.
    for(int tick=-80;tick<160;++tick)for(uint phase=0;phase<7;++phase) {
        float x=mistPairEmission(float(tick),5,0.05,float(phase)/7.0,0);
        float z=mistPairEmission(float(tick),5,0.05,float(phase)/7.0,2);
        if(x<0 || x>1 || abs(x+z-1)>0.00001)return 8;
        if(abs(x-mistPairEmission(float(tick)+40,5,0.05,float(phase)/7.0,0))>0.00001)return 9;
    }
    // Birth-bin refinement must not multiply the total emitted amount.
    float totals[3];
    for(uint choice=0;choice<3;++choice) {
        uint count=6u<<choice;float width=3.0/float(count);float amount=0;
        for(uint n=0;n<count;++n)for(uint nozzle=0;nozzle<4;++nozzle) {
            float age=(float(n)+0.5)*width;
            amount+=mistVisibleAmount(age,width,mistPairEmission(20-age/0.05,width/0.05,0.05,0,nozzle));
        }
        totals[choice]=amount;
    }
    if(abs(totals[0]/totals[2]-1)>0.004 || abs(totals[1]/totals[2]-1)>0.001)return 10;

    // At a useful upper horizontal section the spray occupies arcs, not a
    // uniformly filled disc. Below the source there is a diffuse free-air tail.
    float core=freeDensity(float3(0,0.07,0),20),arc=0;
    for(uint i=0;i<40;++i)for(uint nozzle=0;nozzle<4;++nozzle)
        arc=max(arc,freeDensity(mistFlowDirection(nozzle)*(0.01*float(i))+float3(0,0.07,0),20));
    if(arc<core*2 || arc<0.1)return 11;
    float tail=0;
    for(uint i=0;i<40;++i)for(uint nozzle=0;nozzle<4;++nozzle)
        tail=max(tail,freeDensity(mistFlowDirection(nozzle)*(0.01*float(i))+float3(0,-0.12,0),20));
    if(tail<=0.0 || tail>=arc)return 12;

    // Real swept traversal, not just an endpoint test: a one-voxel wall cannot
    // be skipped even when the endpoint is clear on the other side.
    MistVoxelWalk walk=mistBeginWalk(float3(0.5,2.5,1.5),float3(6.5,2.5,1.5));
    bool hit=false;
    for(uint i=0;i<16;++i) {
        if(walk.cell.x==3) {hit=true;break;}
        if(!mistWalkNext(walk))break;
    }
    if(!hit || abs(walk.entry-2.5/6.0)>0.00001 || any(walk.normal!=float3(-1,0,0)))return 13;
    walk=mistBeginWalk(float3(6.5,2.5,1.5),float3(0.5,2.5,1.5));hit=false;
    for(uint i=0;i<16;++i) {
        if(walk.cell.x==3) {hit=true;break;}
        if(!mistWalkNext(walk))break;
    }
    if(!hit || any(walk.normal!=float3(1,0,0)))return 14;
    walk=mistBeginWalk(float3(0.5),float3(4.5));
    for(uint i=0;i<4;++i) {
        if(!mistWalkNext(walk) || walk.cell.x!=int(i+1) || walk.cell.y!=int(i+1) || walk.cell.z!=int(i+1))return 15;
    }
    walk=mistBeginWalk(float3(1.5),float3(1.5));
    if(mistWalkNext(walk))return 16;

    // Finite-kernel air-side clipping prevents a cloud centred on one side of
    // a thin wall from rendering its support on the other side.
    float2 clipped=float2(0,4);
    if(!mistClipAirPlane(float3(-2,0,0),float3(1,0,0),float3(0),float3(-1,0,0),clipped))return 17;
    if(abs(clipped.y-2)>0.00001)return 18;
    clipped=float2(0,1);
    if(mistClipAirPlane(float3(0.5,0,0),float3(0,0,1),float3(0),float3(-1,0,0),clipped))return 19;
    return 0;
}
