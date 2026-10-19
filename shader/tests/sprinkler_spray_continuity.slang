import sprinkler_animation;
import sprinkler_mist_math;
import sprinkler_mist_transport;
import sprinkler_spray;

static const uint COUNT=256u; // production budget, not a high-resolution substitute
static const uint AGES=COUNT/MIST_ROTARY_BANDS;
struct Prepared { float3 position;float3 radii;float3 tangent;float amplitude; };
float3 centerAtOffset(MistSpraySource source,float offset,float3 target)
{
    // Independent finite-difference reference: retain the birth direction, not
    // the current head angle, when moving along a velocity band's trajectory.
    float angle=-offset*source.angular_velocity;
    float3 p,v;
    mistFlowAdvance(sprinklerRotate(source.outlet,angle),sprinklerRotate(source.velocity,angle),
        target,source.age+offset,p,v);
    return p;
}
float density(float3 point,Prepared lobes[COUNT],uint band)
{
    float sum=0;
    for(uint n=band*AGES;n<(band+1u)*AGES;++n) {
        Prepared lobe=lobes[n];
        float3 q=mistSprayToLocal(point-lobe.position,float4(lobe.tangent,1))/lobe.radii;
        float k=max(0.0,1.0-dot(q,q));
        sum+=k*k*lobe.amplitude;
    }
    return sum;
}

export __extern_cpp int main()
{
    uint failures=0u;float worst=1.0e10;
    float speeds[3]={0,15,60},angles[3]={10,40,90};
    for(uint variant=0u;variant<36u;++variant) {
        float angleNow=float(variant)*0.37,seconds=variant<18u?0.05:0.08,phase=float(variant)/36.0;
        float rpm=speeds[variant%3u],angle=angles[(variant/3u)%3u];
        float3 target=mistSprayTarget(float2(variant%2u==0u?0.0:0.02,0.01),true);
        MistSpraySource sources[COUNT];Prepared lobes[COUNT];
        for(uint n=0;n<COUNT;++n) {
            MistSpraySource s=mistRotarySource(n,COUNT,angleNow,phase,rpm,angle,1,0);
            sources[n]=s;
            lobes[n].position=centerAtOffset(s,0,target);
            lobes[n].tangent=mistSprayAgeTangent(s,target,seconds);
            lobes[n].radii=mistSprayRadii(s,lobes[n].tangent);
            lobes[n].amplitude=mistKernelAmplitude(mistSprayVisibleAmount(s,true),lobes[n].radii);
        }
        float emission=0;
        for(uint band=0;band<MIST_ROTARY_BANDS;++band) {
            float previousEnd=0,totalWidth=0;
            emission+=sources[band*AGES].emission;
            for(uint age=0;age<AGES;++age) {
                uint n=band*AGES+age;MistSpraySource s=sources[n];
                if(s.nozzle!=0u || s.emission<=0 || s.width<=0 || abs(s.age-0.5*s.width-previousEnd)>1e-5)failures++;
                if(length(s.outlet-sources[age].outlet)>1e-6)failures++; // one mouth, not eight nozzles
                previousEnd=s.age+0.5*s.width;totalWidth+=s.width;
                float3 tangent=lobes[n].tangent,r=lobes[n].radii;
                float3 finiteDifference=(centerAtOffset(s,0.002,target)-centerAtOffset(s,-0.002,target))/0.004;
                if(length(tangent-finiteDifference)>0.0003)failures++;
                float4 frame=float4(tangent,1);
                float3 x,y,z;mistSprayBasis(frame,x,y,z);
                float3 bounds=mistSprayBoundsRadius(r,frame);
                for(uint corner=0u;corner<8u;++corner) {
                    float3 p=x*r.x*((corner&1u)!=0u?1.0:-1.0)+y*r.y*((corner&2u)!=0u?1.0:-1.0)+z*r.z*((corner&4u)!=0u?1.0:-1.0);
                    if(any(abs(p)>bounds+1e-6))failures++;
                }
                if(abs(dot(x,y))+abs(dot(y,z))+abs(dot(z,x))>1e-5)failures++;
                float3 origin=-y*r.y*2.0,dir=y;
                float3 localOrigin=mistSprayToLocal(origin,frame),localDir=mistSprayToLocal(dir,frame);
                float2 interval;
                if(!mistRayInterval(localOrigin,localDir,float3(0),r,4.0*r.y,interval))failures++;
                else if(abs(mistKernelLineIntegral(localOrigin,localDir,float3(0),r,interval)/(16.0*r.y/15.0)-1.0)>0.001)failures++;
                if(age>=3u && age+1u<AGES && lobes[n+1u].position.y>0) {
                    float offset=(sources[n+1u].age-s.age)*0.5;
                    float average=0.5*(density(lobes[n].position,lobes,band)+density(lobes[n+1u].position,lobes,band));
                    float ratio=density(centerAtOffset(s,offset,target),lobes,band)/average;
                    worst=min(worst,ratio);
                    if(!isfinite(ratio) || ratio<0.6) {
                        printf("gap rpm=%.0f angle=%.0f band=%u age=%.5f density_ratio=%.5f\n",rpm,angle,band,s.age,ratio);
                        failures++;
                    }
                }
            }
            if(abs(totalWidth-MIST_FLOW_LIFETIME)>1e-5)failures++;
            float3 outlet=sprinklerRotaryOutlet(angleNow+phase*6.28318530718);
            if(density(outlet,lobes,band)<=0.0)failures++;
            float3 r0=lobes[band*AGES].radii,r1=lobes[band*AGES+AGES/2].radii;
            // Sub-voxel mouth support (half a voxel through the default angle).
            // The baffle opens an anisotropic sheet, so compare transverse AREA
            // rather than requiring the slower bands to grow equally on both axes.
            float mouthLimit=(angle>40?1.0:0.5)/256.0;
            if(r0.y>mouthLimit || r0.z>mouthLimit || r1.y*r1.z<r0.y*r0.z*64)failures++;
            if(centerAtOffset(sources[band*AGES],2.5,target).y>=outlet.y)failures++;
        }
        if(abs(emission-2)>1e-5)failures++;
    }
    float amounts[3];
    for(uint c=0u;c<3u;++c) {
        uint count=COUNT<<c;float amount=0;
        for(uint n=0;n<count;++n)amount+=mistSprayVisibleAmount(mistSpraySource(n,count,20,0.05,0,true),true);
        amounts[c]=amount;
    }
    printf("optical amount refinement %.6f / %.6f\n",amounts[0]/amounts[2],amounts[1]/amounts[2]);
    if(abs(amounts[0]/amounts[2]-1)>0.002 || abs(amounts[1]/amounts[2]-1)>0.001)failures++;
    printf("single-mouth deflected spray: density floor %.4f (required >=0.6), failures=%u\n",worst,failures);
    return int(failures!=0u);
}
