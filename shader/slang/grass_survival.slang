// Terrain edits invalidate observations, not grass identities. The root voxel
// is support; one or two solid voxels above it are tolerated, three are not.
module grass_survival;
import voxel_types;
public bool grassSurvivesTerrain(uint support,uint above1,uint above2,uint above3) {
    if((support&15u)==VOXEL_TYPE_EMPTY)return false;
    return (above1&15u)==VOXEL_TYPE_EMPTY ||
           (above2&15u)==VOXEL_TYPE_EMPTY ||
           (above3&15u)==VOXEL_TYPE_EMPTY;
}
