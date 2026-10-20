import sprinkler_spray;
import sprinkler_mist_transport;

export __extern_cpp int main()
{
    uint failures=0,advected=0;
    for(uint fi=0;fi<5u;++fi) {
        float hz=fi==0u?0.25:(fi==1u?1.0:(fi==2u?4.0:(fi==3u?9.0:20.0)));
        for(uint d=0;d<=10u;++d) {
            float duty=float(d)*0.1,sum=0.0,period=1.0/hz;
            for(uint n=0;n<240u;++n)
                sum+=sprinklerPulseAmount((float(n)+0.5)*period/240.0,period/240.0,duty,hz)/240.0;
            if(abs(sum-duty)>0.0001)failures++;
            for(uint n=0;n<128u;++n) {
                float dt=0.013,omega=15.0*6.28318530718/60.0;
                MistSpraySource a=mistDropletSource(n,128u,0.47,0.3,0.125,15,40,duty,0.13,hz);
                MistSpraySource b=mistDropletSource(n,128u,0.47+dt,0.3+dt*omega,0.125,15,40,duty,0.13+dt,hz);
                if(!all(isfinite(a.velocity)) || !all(isfinite(a.outlet)))failures++;
                if(d==0u && a.emission!=0.0)failures++;
                if(d==10u && a.emission!=1.0)failures++;
                if(b.age<a.age)continue; // a slot's next birth, not an old drop
                advected++;
                if(abs(b.age-a.age-dt)>0.000001 ||
                   length(b.velocity-a.velocity)>0.00001 || length(b.outlet-a.outlet)>0.000001 ||
                   a.emission!=b.emission)failures++;
                float3 p,v,next,nv,expected,ev,target=mistSprayTarget(float2(0.01,-0.01),true);
                mistFlowAdvance(a.outlet,a.velocity,target,a.age,p,v);
                mistFlowAdvance(p,v,target,dt,next,nv);
                mistFlowAdvance(b.outlet,b.velocity,target,b.age,expected,ev);
                if(length(next-expected)>0.000001)failures++;
            }
        }
    }
    printf("droplet birth/advection and 0.25-20Hz valve contracts: cases=%u failures=%u\n",advected,failures);
    return int(failures!=0u);
}
