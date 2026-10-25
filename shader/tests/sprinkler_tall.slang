import sprinkler_spray;
import sprinkler_mist_transport;

export __extern_cpp int main()
{
    uint failures=0;
    float peaks[2],ranges[2];
    for(uint kind=0;kind<2;++kind) {
        float peak=0,range=0,flux=0;
        bool falling=false;
        for(uint band=0;band<8;++band) {
            MistSpraySource s=mistRotarySource(band*32,256,0,0,0,30,1,0,4,0.5,0.65,kind);
            flux+=s.emission;
            float3 p,v;
            for(uint step=0;step<360;++step) {
                float age=float(step)/120.0;
                mistFlowAdvance(s.outlet,s.velocity,mistSprayTarget(float2(0),true),age,p,v);
                if(p.y<0)break;
                peak=max(peak,p.y);range=max(range,length(p.xz));
                falling=falling||v.y<-(kind==1?0.25:0.1);
            }
            if(abs(s.outlet.y-(4.2+(kind==1?22.0:0.0))*0.65/256.0)>1e-6)failures++;
        }
        if(abs(flux-(kind==1?3.6:2.0))>1e-5 || !falling)failures++;
        peaks[kind]=peak;ranges[kind]=range;
        printf("kind=%u peak=%f range=%f total_flux=%f falling=%u\n",kind,peak,range,flux,uint(falling));
        for(uint n=0;n<128;++n) {
            MistSpraySource s=mistDropletSource(n,128,1.0,0,0,5,30,1,0,4,0.65,kind);
            if(s.outlet.y<(kind==1?0.06:0.0) || !all(isfinite(s.velocity)))failures++;
        }
    }
    if(peaks[1]<peaks[0]*2 || ranges[1]<ranges[0]*1.5)failures++;
    printf("tall sprinkler failures=%u\n",failures);
    return int(failures!=0);
}
