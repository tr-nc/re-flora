import sprinkler_animation;
import sprinkler_mist_transport;

export __extern_cpp int main()
{
    uint failures=0;
    // Whole hardware pose selection uses nearest, wrapped 16-way articulation.
    for(int frame=-32;frame<=32;++frame)
        for(int offset=-4;offset<=4;++offset) {
            float angle=(float(frame)+float(offset)*0.1)*6.28318530718/16.0;
            if(sprinklerRotaryFrame(angle)!=uint((frame%16+16)%16))failures++;
        }
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
        if(length(sprinklerRotate(float3(1,0,0),a)-d)<0.1)failures++;
        if(abs(p.y-4.2/256.0)>1e-7 || abs(length(d)-1.0)>1e-6 || abs(dot(d,older))>1e-5)failures++;
        if(length(p-sprinklerRotaryOutlet(a))>1e-7 || length(d-sprinklerRotaryDirection(a))>1e-7)failures++;
    }
    // Irrigation contracts now live in sprinkler_spray_contact.slang: there is
    // deliberately no independent rotary sector/radius model left to test.
    printf("rotary hardware contracts failures=%u\n",failures);
    return int(failures!=0u);
}
