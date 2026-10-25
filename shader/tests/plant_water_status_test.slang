import plant_water_status;

export __extern_cpp int main() {
    for(uint thresholdIndex=1u;thresholdIndex<=4u;thresholdIndex++) {
        float threshold=float(thresholdIndex)/4.0;
        float previous=2.0;
        for(uint level=0u;level<=16u;level++) {
            float hydration=float(level)/16.0;
            float stress=plantStoredWaterDeficit(hydration,threshold);
            if(stress<0.0||stress>1.0||stress>previous)return 1;
            if(level==0u&&stress!=1.0)return 2;
            if(hydration>=threshold&&stress!=0.0)return 3;
            previous=stress;
            for(uint seed=0u;seed<64u;seed++) {
                if(any(plantWiltOffset(stress,32.0,0.0,seed,1.0)!=float3(0)))return 4;
                float3 tip=plantWiltOffset(stress,32.0,1.0,seed,1.0);
                if(length(tip)>32.0*0.25||tip.y>0.0||any(isnan(tip)))return 5;
                if(level==0u&&(tip.y>=0.0||length(tip.xz)<=0.0))return 6;
                if(hydration>=threshold&&any(tip!=float3(0)))return 7;
                if(any(plantWiltOffset(stress,32.0,1.0,seed,0.0)!=float3(0)))return 8;
                if(length(plantWiltOffset(stress,32.0,1.0,seed,0.5)-tip*0.5)>1e-6)return 9;
                if(any(plantWiltOffset(stress,32.0,1.0,seed,2.0)!=tip))return 10;
            }
        }
    }
    return 0;
}
