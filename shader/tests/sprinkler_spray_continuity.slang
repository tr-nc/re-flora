import sprinkler_animation;
import sprinkler_mist_math;
import sprinkler_mist_transport;
import sprinkler_spray;

static const uint COUNT=48u; // production frame budget, not a high-resolution test substitute
float3 centerAtAge(float age,float tick,float seconds,float phase,float3 target)
{
    float birth=tick-age/seconds;
    float3 p,v;
    mistFlowAdvance(sprinklerRotaryOutletAtTick(birth,seconds,phase),
        mistSprayLaunchVelocity(birth,seconds,phase),target,age,p,v);
    return p;
}
float density(float3 point,float tick,float seconds,float phase,float3 target)
{
    float sum=0;
    for(uint n=0u;n<COUNT;++n) {
        MistSpraySource s=mistSpraySource(n,COUNT,tick,seconds,phase,true);
        float3 p,v;mistFlowAdvance(s.outlet,s.velocity,target,s.age,p,v);
        float3 tangent=mistSprayAgeTangent(s,target,seconds),r=mistSprayRadii(s,tangent);
        float3 q=mistSprayToLocal(point-p,float4(tangent,1))/r;
        float k=max(0.0,1.0-dot(q,q));
        sum+=k*k*mistKernelAmplitude(mistSprayVisibleAmount(s,true),r);
    }
    return sum;
}

export __extern_cpp int main()
{
    uint failures=0u;
    float worst=1.0e10;
    for(uint variant=0u;variant<12u;++variant) {
        float tick=float(variant)*6.7,seconds=variant<6u?0.05:0.08,phase=float(variant)/12.0;
        float3 target=mistSprayTarget(float2(variant%2u==0u?0.0:0.02,0.01),true);
        float previousEnd=0.0,totalWidth=0.0;
        for(uint n=0u;n<COUNT;++n) {
            MistSpraySource s=mistSpraySource(n,COUNT,tick,seconds,phase,true);
            if(s.nozzle!=0u || s.emission!=2.0 || s.width<=0.0 || abs(s.age-0.5*s.width-previousEnd)>1e-5)failures++;
            previousEnd=s.age+0.5*s.width;totalWidth+=s.width;
            float3 tangent=mistSprayAgeTangent(s,target,seconds),r=mistSprayRadii(s,tangent);
            float3 finiteDifference=(centerAtAge(s.age+0.004,tick,seconds,phase,target)-centerAtAge(s.age-0.004,tick,seconds,phase,target))/0.008;
            if(length(tangent-finiteDifference)>0.0003)failures++;
            float4 frame=float4(tangent,1);
            float3 x,y,z;mistSprayBasis(frame,x,y,z);
            float3 bounds=mistSprayBoundsRadius(r,frame);
            for(uint corner=0u;corner<8u;++corner) {
                float3 p=x*r.x*((corner&1u)!=0u?1.0:-1.0)+y*r.y*((corner&2u)!=0u?1.0:-1.0)+z*r.z*((corner&4u)!=0u?1.0:-1.0);
                if(any(abs(p)>bounds+1e-6))failures++;
            }
            if(abs(dot(x,y))+abs(dot(y,z))+abs(dot(z,x))>1e-5)failures++;
            // Exact integration remains valid after the frame rotation and depth clipping.
            float3 origin=-y*r.y*2.0,dir=y;
            float3 localOrigin=mistSprayToLocal(origin,frame),localDir=mistSprayToLocal(dir,frame);
            float2 interval;
            if(!mistRayInterval(localOrigin,localDir,float3(0),r,4.0*r.y,interval))failures++;
            else if(abs(mistKernelLineIntegral(localOrigin,localDir,float3(0),r,interval)/(16.0*r.y/15.0)-1.0)>0.001)failures++;
            if(n>=6u && n+1u<COUNT) {
                float next=mistSpraySource(n+1u,COUNT,tick,seconds,phase,true).age;
                float mid=(s.age+next)*0.5;
                float average=0.5*(density(centerAtAge(s.age,tick,seconds,phase,target),tick,seconds,phase,target)+
                    density(centerAtAge(next,tick,seconds,phase,target),tick,seconds,phase,target));
                float ratio=density(centerAtAge(mid,tick,seconds,phase,target),tick,seconds,phase,target)/average;
                worst=min(worst,ratio);
                if(!isfinite(ratio) || ratio<0.6)failures++;
            }
        }
        if(abs(totalWidth-MIST_FLOW_LIFETIME)>1e-5)failures++;
        // Spray begins at the mouth, not at the center of a detached first puff.
        float3 outlet=sprinklerRotaryOutletAtTick(tick,seconds,phase);
        if(density(outlet,tick,seconds,phase,target)<=0.0)failures++;
        // Millimetric mouth support opens with distance; it isn't a source-sized cloud.
        MistSpraySource young=mistSpraySource(0u,COUNT,tick,seconds,phase,true);
        MistSpraySource older=mistSpraySource(20u,COUNT,tick,seconds,phase,true);
        float3 r0=mistSprayRadii(young,mistSprayAgeTangent(young,target,seconds));
        float3 r1=mistSprayRadii(older,mistSprayAgeTangent(older,target,seconds));
        if(r0.y>0.001 || r0.z>0.001 || r1.y<r0.y*8.0 || r1.z<r0.z*8.0)failures++;
        if(centerAtAge(2.5,tick,seconds,phase,target).y>=outlet.y)failures++;
    }
    float amounts[3];
    for(uint c=0u;c<3u;++c) {
        uint count=24u<<c;float amount=0.0;
        for(uint n=0u;n<count;++n) {
            MistSpraySource s=mistSpraySource(n,count,20,0.05,0,true);
            amount+=mistSprayVisibleAmount(s,true);
        }
        amounts[c]=amount;
    }
    printf("optical amount refinement %.6f / %.6f\n",amounts[0]/amounts[2],amounts[1]/amounts[2]);
    if(abs(amounts[0]/amounts[2]-1)>0.002 || abs(amounts[1]/amounts[2]-1)>0.001)failures++;
    printf("single-nozzle spray: density floor %.4f (required >=0.6), failures=%u\n",worst,failures);
    return int(failures!=0u);
}
