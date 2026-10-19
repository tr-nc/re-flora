import sprinkler_animation;
import sprinkler_mist_transport;
import sprinkler_spray;
import sprinkler_spray_contact;

// Same source/flight/contact interface as native preparation. On flat, still
// ground one revolution equals averaging the footprint around a circular ring.
static const uint COUNT=256u;
float groundContact(MistSpraySource source,float3 target,out float3 point)
{
    float lo=0,hi=MIST_FLOW_LIFETIME;float3 velocity;
    for(uint i=0;i<26;++i) {
        float age=(lo+hi)*0.5;
        mistFlowAdvance(source.outlet,source.velocity,target,age,point,velocity);
        if(point.y>0)lo=age;else hi=age;
    }
    mistFlowAdvance(source.outlet,source.velocity,target,hi,point,velocity);
    point.y=0;return hi;
}

export __extern_cpp int main()
{
    uint failures=0;
    float speeds[3]={0,15,60},angles[3]={10,40,90};
    float3 target=mistSprayTarget(float2(0),true);
    float worstRotationError=0;
    for(uint speed=0;speed<3;++speed)for(uint angle=0;angle<3;++angle) {
        B_SprinklerWaterContact contacts[32];uint count=0;
        for(uint n=0;n<COUNT;++n) {
            MistSpraySource source=mistRotarySource(n,COUNT,0,0,speeds[speed],angles[angle],1,0);
            float3 point;float age=groundContact(source,target,point);
            if(!sprinklerArrivesInBin(age,source))continue;
            if(count>=32)return 2;
            B_SprinklerWaterContact contact=sprinklerWaterContact(source,point,float3(0,1,0),target,0.05,age);
            contacts[count++]=contact;
            // Steep near trajectories must rotate with the head, not depend on
            // an arbitrary world axis chosen to construct their kernel frame.
            MistSpraySource rotated=mistRotarySource(n,COUNT,0.7,0,speeds[speed],angles[angle],1,0);
            float3 other;float otherAge=groundContact(rotated,target,other);
            B_SprinklerWaterContact turned=sprinklerWaterContact(rotated,other,float3(0,1,0),target,0.05,otherAge);
            for(int z=-4;z<=4;++z)for(int x=-4;x<=4;++x) {
                float3 p=point+float3(float(x)*0.005,-0.5/256.0,float(z)*0.005);
                float before=sprinklerContactDensity(contact,p);
                float after=sprinklerContactDensity(turned,sprinklerRotate(p,0.7));
                worstRotationError=max(worstRotationError,abs(after-before)/max(1.0,max(before,after)));
            }
        }
        float radii[7]={0.012,0.02,0.05,0.10,0.15,0.22,0.28},mean[7];
        for(uint band=0;band<7;++band) {
            float sum=0;
            for(uint sample=0;sample<512;++sample) {
                float azimuth=(float(sample)+0.5)*6.28318530718/512.0;
                float3 receiver=float3(radii[band]*cos(azimuth),-0.5/256.0,radii[band]*sin(azimuth));
                for(uint n=0;n<count;++n)sum+=sprinklerContactDensity(contacts[n],receiver);
            }
            mean[band]=sum/512.0;
            printf("rpm=%.0f angle=%.0f radius=%.3f revolution_mean_input=%.6f\n",speeds[speed],angles[angle],radii[band],mean[band]);
        }
        float reference=(mean[4]+mean[5]+mean[6])/3.0;
        for(uint band=0;band<7;++band) {
            // The innermost sample is at the physical mouth/base footprint;
            // permit its feathered edge, but never a dead center-sized ring.
            float floor=band==0?0.1:0.25;
            if(!isfinite(mean[band]) || mean[band]<=0 || mean[band]<reference*floor)failures++;
            // Area-weighted source discharge avoids solving the dry center by
            // dumping most water there. Allow smooth inner/outer edge falloff.
            if(angles[angle]==40 && mean[band]>reference*2.0)failures++;
        }
    }
    if(worstRotationError>0.0001)failures++;
    printf("radial coverage: rotation_error=%.6f failures=%u (near/middle/far must all receive water)\n",worstRotationError,failures);
    return int(failures!=0);
}
