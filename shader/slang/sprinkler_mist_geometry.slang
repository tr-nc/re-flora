module sprinkler_mist_geometry;

import ddgi_types;
import ddgi_voxel_visibility;
import sprinkler_mist_transport;

public bool mistGeometryReady()
{
    U_DdgiVoxelVisibilityInfo info=ddgiPublishedVoxelGeometryInfo();
    return info.ready!=0u && all(info.world_to_voxel_scale>float3(0)) &&
           all(info.voxel_dimensions>uint3(0));
}

// Terrain only: the published occupancy deliberately excludes animated trees.
// Unknown geometry and exhausted work budgets never mean unobstructed air.
public bool mistSweepTerrain(float3 start,float3 end,out float fraction,out float3 normal)
{
    fraction=0;normal=float3(0);
    if(!mistGeometryReady())return true;
    U_DdgiVoxelVisibilityInfo info=ddgiPublishedVoxelGeometryInfo();
    float3 scale=info.world_to_voxel_scale;
    MistVoxelWalk w=mistBeginWalk(start*scale,end*scale);
    for(uint step=0;step<128u;++step) {
        if(ddgiVoxelIsOccupied(w.cell,info.voxel_dimensions)) {
            fraction=w.entry;normal=w.normal;
            if(dot(normal,normal)>0.0)normal=normalize(normal*scale);
            return true;
        }
        if(!mistWalkNext(w)) { fraction=1.0;return false; }
    }
    return true;
}

public bool mistLobeNearTerrain(float3 center,float3 radii)
{
    return ddgiVoxelRegionMayBeOccupied(center-radii,center+radii);
}

public bool mistConstrainMotion(float3 start,inout float3 end,inout float3 velocity)
{
    float3 p=start;
    float3 scale=ddgiPublishedVoxelGeometryInfo().world_to_voxel_scale;
    float guard=0.02/max(scale.x,max(scale.y,scale.z));
    for(uint contact=0;contact<3u;++contact) {
        float fraction;float3 normal;
        if(!mistSweepTerrain(p,end,fraction,normal))return true;
        if(dot(normal,normal)==0.0)return false;
        float3 hit=lerp(p,end,fraction);
        float3 remaining=end-hit;
        p=hit+normal*guard;
        remaining-=normal*min(0.0,dot(remaining,normal));
        velocity-=normal*min(0.0,dot(velocity,normal));
        end=p+remaining;
    }
    // Multiple-corner contact: deposit rather than tunnel or invent a direction.
    end=p;
    float fraction;float3 normal;
    return !mistSweepTerrain(p,p,fraction,normal);
}

// Finite support matters: a centre can be on the correct side of a thin wall
// while its density kernel extends through it. Clip each visible kernel against
// a locally encountered terrain face, never a global sprinkler-base plane.
public bool mistClipLobeToTerrain(float3 origin,float3 direction,float3 center,
                                 inout float2 interval)
{
    float3 receiver=origin+direction*((interval.x+interval.y)*0.5);
    float fraction;float3 normal;
    if(!mistSweepTerrain(center,receiver,fraction,normal))return true;
    if(dot(normal,normal)==0.0)return false;
    float3 face=lerp(center,receiver,fraction);
    return mistClipAirPlane(origin,direction,face,normal,interval);
}
