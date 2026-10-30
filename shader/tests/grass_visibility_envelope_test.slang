// Run explicitly: slangc this-file -std 2025 -I shader/slang -target executable -o /tmp/grass-bounds && /tmp/grass-bounds
// Exercises production deformation and triangle helpers, not a Rust mirror.
import grass_visibility_bounds;
import flora_types;
import tracer_types;
import flora_motion;
import grass_growth_shape;
import plant_water_status;
import stem_band_geometry;

float3 centerAt(uint band, uint height, uint grown, uint seed, float3 wind,
    bool vibration, U_GuiInput gui, U_FloraGrowthInfo growth) {
    float fraction = float(band) / max(float(height) - 1.0, 1.0);
    float3 offset = wind * saturate(fraction);
    offset += floraNaturalRestBend(gui, seed, fraction, float(height));
    if (vibration) offset += floraWindVibration(gui, wind, fraction, seed, int3(0,band,0), 13.37);
    float liveHeight = max(float(grown) - 1.0, 1.0);
    offset += plantWiltOffset(1.0, liveHeight, float(band)/liveHeight, seed, 1.0);
    offset += sampleFloraSpawnAnimation(growth, 1000u, seed, float(height)).translation_voxels;
    return (float3(0,band,0) + offset + 0.5) / 256.0;
}
export __extern_cpp int main() {
    U_GuiInput gui = {};
    gui.grass_vibration_primary_speed = 37.0;
    gui.grass_vibration_secondary_speed = 71.0;
    U_FloraGrowthInfo growth = {};
    growth.spawn_duration_seconds = 0.28;
    growth.spawn_rise_fraction = 0.6;
    growth.spawn_stagger_seconds = 0.1;
    float4x4 identity = float4x4(1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1);
    if (!grassBoundsVisible(identity,float3(0,0,0.5),float3(0.01))) return 1;
    if (!grassBoundsVisible(identity,float3(0,0,-0.1),float3(0.2))) return 2;
    if (!grassBoundsVisible(identity,float3(0),float3(2))) return 3;
    for (uint axis=0;axis<3;axis++) for (int sign=-1;sign<=1;sign+=2) {
        float3 p=float3(0); p[axis]=float(sign)*2.0;
        if (grassBoundsVisible(identity,p,float3(0.01))) return 4;
    }
    uint points=0;
    for (uint species=0;species<2;species++) for (uint seed=0;seed<32;seed++)
    for (uint preset=0;preset<5;preset++) for (uint tick=0;tick<7;tick++)
    for (uint fraction=1;fraction<=4;fraction++) for (uint vibrate=0;vibrate<2;vibrate++) {
        uint bands=species==0?8u:4u;
        uint height=sampleGrassHeight(species,seed);
        uint grown=max(1u,height*fraction/4u);
        float force = preset==0?0.0:(preset==1?2.0:(preset==2?-50.0:(preset==3?1000.0:-1000.0)));
        float3 wind=float3(force,0,-force*0.37);
        gui.grass_natural_bend_max_voxels=abs(force);
        gui.grass_vibration_amplitude_voxels=force*0.2;
        growth.spawn_time_ms=1000u+tick*70u;
        growth.spawn_overshoot_min_voxels=-abs(force);
        growth.spawn_overshoot_max_voxels=abs(force);
        float3 extent=grassVisibilityExtent(bands,wind,gui,growth);
        for (uint band=0;band<grown;band++) {
            float3 center=centerAt(band,height,grown,seed,wind,vibrate!=0,gui,growth);
            float3 previous=band>0?centerAt(band-1,height,grown,seed,wind,vibrate!=0,gui,growth):center;
            float3 next=band+1<grown?centerAt(band+1,height,grown,seed,wind,vibrate!=0,gui,growth):center;
            float3 a,b,ta,tb;
            stemBandCenterFrames(center,previous,next,band>0,band+1<grown,0.5/256.0,a,b,ta,tb);
            for (uint corner=0;corner<8;corner++) {
                float3 point=stemBandPoint(a,b,ta,tb,float3(1,0,0),float3(1,0,0),float2(0.5/256.0),float2(0.5/256.0),stemBandCorner(corner));
                if (any(abs(point)>extent+1e-6)) { printf("envelope failed species=%u seed=%u preset=%u\n",species,seed,preset); return 5; }
                // A visible real vertex must never be rejected, at every frustum edge.
                for(uint axis=0;axis<3;axis++) for(int sign=-1;sign<=1;sign+=2) {
                    float3 root=float3(0,0,0.5);root[axis]=float(sign)*0.99;
                    float3 p=point+root;
                    bool inClip=abs(p.x)<=1.0&&abs(p.y)<=1.0&&p.z>=0.0&&p.z<=1.0;
                    if(inClip&&!grassBoundsVisible(identity,root,extent))return 6;
                }
                points++;
            }
        }
    }
    printf("PASS: %u actual stem vertices, growth/wilt/spawn/wind envelopes and all six frustum planes\n",points);
    return 0;
}
