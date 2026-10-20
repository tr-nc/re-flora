import sprinkler_spray;
import sprinkler_spray_contact;
import sprinkler_mist_transport;

// Exact descending crossings of concentric downward treads. Test the first
// valid plane, not a global height-field bisection that can skip a narrow tread.
float firstTread(MistSpraySource s,float stepHeight,float stepWidth)
{
    float3 target=mistSprayTarget(float2(0),true),p,v;
    for(uint level=0;level<64u;++level) {
        float height=-float(level)*stepHeight/256.0;
        float age=(s.outlet.y-height+(s.velocity.y-target.y)*MIST_FLOW_DRAG_TIME)/(-target.y);
        for(uint i=0;i<7u;++i) {
            mistFlowAdvance(s.outlet,s.velocity,target,age,p,v);
            age-=(p.y-height)/min(v.y,-0.00001);
        }
        mistFlowAdvance(s.outlet,s.velocity,target,age,p,v);
        if(uint(floor(length(p.xz)/stepWidth))==level)return age;
    }
    return 3.0;
}

export __extern_cpp int main()
{
    uint failures=0;
    float3 target=mistSprayTarget(float2(0),true);
    for(uint geometry=0;geometry<3u;++geometry) {
        float stepHeight=float(geometry),stepWidth=0.025;
        float means[13];for(uint r=0;r<13u;++r)means[r]=0;
        for(uint epoch=0;epoch<64u;++epoch) {
            float quadrature=(float(epoch)+0.5)/64.0;
            B_SprinklerWaterContact contacts[8];uint count=0;float mass=0;
            for(uint band=0;band<8u;++band) {
                MistSpraySource seed=mistRotarySource(band*32u,256u,0,0,0,40,1,0,4,quadrature);
                float age=firstTread(seed,stepHeight,stepWidth);
                uint bin=uint(floor(sqrt(age/3.0)*32.0));
                MistSpraySource s=mistRotarySource(band*32u+bin,256u,0,0,0,40,1,0,4,quadrature);
                if(!sprinklerArrivesInBin(age,s)) { failures++;continue; }
                float3 p,v;mistFlowAdvance(s.outlet,s.velocity,target,age,p,v);
                contacts[count]=sprinklerWaterContact(s,p,float3(0,1,0),target,0.05,age);
                mass+=contacts[count++].center_flux.w;
            }
            if(count!=8u || abs(mass-1.0)>0.00001)failures++;
            for(uint ring=0;ring<13u;++ring) {
                float radius=(float(ring)+0.5)*stepWidth;
                for(uint a=0;a<128u;++a) {
                    float angle=float(a)*6.28318530718/128.0;
                    float3 p=float3(cos(angle)*radius,-(float(ring)*stepHeight+0.5)/256.0,sin(angle)*radius);
                    for(uint c=0;c<count;++c)means[ring]+=sprinklerContactDensity(contacts[c],p)/(128.0*64.0);
                }
            }
        }
        uint dry=0;
        for(uint ring=0;ring<13u;++ring) {
            if(!isfinite(means[ring]) || means[ring]<0.01)dry++;
            // A tiny noisy contribution must not disguise the old dark/light rings.
            if(ring>0u && ring<12u && means[ring]<0.15*(means[ring-1u]+means[ring+1u])*0.5)failures++;
        }
        failures+=dry;
        if(geometry==0u) {
            float reference=(means[6]+means[8]+means[10])/3.0;
            for(uint r=0;r<11u;++r)if(means[r]>2.0*reference)failures++;
        }
        printf("terraces step=%u voxel(s): dry=%u near=%.4f middle=%.4f far=%.4f\n",geometry,dry,means[0],means[6],means[12]);
    }
    float hits[64][8];
    for(uint q=0;q<64u;++q)for(uint b=0;b<8u;++b) {
        MistSpraySource s=mistRotarySource(b*32u,256u,0,0,0,40,1,0,4,(float(q)+0.5)/64.0);
        hits[q][b]=firstTread(s,0,1.0);
    }
    float worstDoseError=0.0;
    for(uint rate=0;rate<3u;++rate)for(uint f=0;f<5u;++f)for(uint d=0;d<3u;++d) {
        uint fps=30u<<rate,frames=32u*fps;
        float hz=f==0u?0.25:(f==1u?1.0:(f==2u?4.0:(f==3u?9.0:20.0)));
        float duty=d==0u?0.1:(d==1u?0.3:0.5),dose=0;
        for(uint frame=0;frame<frames;++frame) {
            float time=(float(frame)+0.5)/float(fps);
            uint epoch=uint(floor(time*60.0));
            uint q=((epoch*0x9e3779b9u)>>26u)+8u;q%=64u;
            for(uint band=0;band<8u;++band) {
                uint bin=uint(floor(sqrt(hits[q][band]/3.0)*32.0));
                MistSpraySource s=mistRotarySource(band*32u+bin,256u,0,0.125,15,40,duty,
                    frac(time*hz)/hz,hz,(float(q)+0.5)/64.0);
                dose+=s.emission*0.5/float(frames);
            }
        }
        worstDoseError=max(worstDoseError,abs(dose-duty));
        if(abs(dose-duty)>0.02)failures++;
    }
    printf("temporal valve dose worst_error=%.6f (30/60/120Hz, 0.25-20Hz valves)\n",worstDoseError);
    printf("temporal velocity coverage failures=%u\n",failures);
    return int(failures!=0u);
}
