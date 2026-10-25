import plant_water_status;
import flora_types;
import voxel_types;

export __extern_cpp int main() {
    for(uint species=0u;species<FLORA_SPECIES_COUNT;species++) {
        float previous=2.0;
        for(uint level=0u;level<4u;level++) {
            uint soil=VOXEL_TYPE_DIRT|(level<<4u);
            float stress=plantWaterDeficit(species,soil,true);
            if(stress<0.0||stress>1.0||stress>previous)return 1;
            if(level==0u&&stress!=1.0)return 2;
            if(level==3u&&stress!=0.0)return 3;
            previous=stress;
            for(uint seed=0u;seed<64u;seed++) {
                if(any(plantWiltOffset(stress,32.0,0.0,seed,1.0)!=float3(0)))return 4;
                float3 tip=plantWiltOffset(stress,32.0,1.0,seed,1.0);
                if(length(tip)>32.0*0.25||tip.y>0.0||any(isnan(tip)))return 5;
                if(level==0u&&(tip.y>=0.0||length(tip.xz)<=0.0))return 6;
                if(level==3u&&any(tip!=float3(0)))return 7;
                if(any(plantWiltOffset(stress,32.0,1.0,seed,0.0)!=float3(0)))return 13;
                if(length(plantWiltOffset(stress,32.0,1.0,seed,0.5)-tip*0.5)>1e-6)return 14;
                if(any(plantWiltOffset(stress,32.0,1.0,seed,2.0)!=tip))return 15;
            }
        }
        if(plantWaterDeficit(species,VOXEL_TYPE_DIRT,false)!=0.0)return 8;
        if(plantWaterDeficit(species,0u,true)!=0.0)return 9;
        // Wet -> dry -> wet returns exactly to the original pose, without history.
        float wet=plantWaterDeficit(species,VOXEL_TYPE_DIRT|0x30u,true);
        if(any(plantWiltOffset(wet,32.0,1.0,123u,1.0)!=float3(0)))return 10;
    }
    if(plantWaterDeficit(FLORA_SPECIES_TREE_LEAF,VOXEL_TYPE_DIRT,true)!=0.0)return 11;
    if(plantWaterDeficit(FLORA_SPECIES_APPLE,VOXEL_TYPE_DIRT,true)!=0.0)return 12;
    return 0;
}
