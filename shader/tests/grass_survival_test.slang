import grass_survival;
import voxel_types;
import flora_occupancy;
export __extern_cpp int main() {
    uint soil=VOXEL_TYPE_DIRT;
    // Support loss always wins, including no cover or a floating roof.
    if(grassSurvivesTerrain(0,0,0,0))return 1;
    if(grassSurvivesTerrain(0,soil,soil,soil))return 2;
    if(!grassSurvivesTerrain(soil,0,0,0))return 3;
    if(!grassSurvivesTerrain(soil,soil,0,0))return 4;
    if(!grassSurvivesTerrain(soil,soil,soil,0))return 5;
    if(grassSurvivesTerrain(soil,soil,soil,soil))return 6;
    // Non-contiguous overhead material is not three layers of burial.
    if(!grassSurvivesTerrain(soil,soil,0,soil))return 7;
    if(!grassSurvivesTerrain(soil,0,soil,soil))return 8;
    // Moisture/fertility bits are not geometry.
    for(uint state=0;state<256u;state+=16u) {
        if(!grassSurvivesTerrain(soil|state,soil|state,soil|state,state))return 9;
        if(grassSurvivesTerrain(soil|state,soil|state,soil|state,soil|state))return 10;
    }
    for(uint growth=0u;growth<=255u;growth++) {
        uint packed=packFloraOccupancyWithSpawnAge(1u,growth,1234u)|FLORA_OCCUPANCY_EXISTING_BIT;
        if(unpackFloraOccupancyGrowth(packed)!=growth ||
           unpackFloraOccupancySelection(packed)!=1u ||
           unpackFloraOccupancySpawnAgeMs(packed)!=1234u)return 11;
    }
    return 0;
}
