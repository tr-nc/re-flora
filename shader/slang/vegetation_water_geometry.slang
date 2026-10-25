module vegetation_water_geometry;
import vegetation_water_types;
import ddgi_voxel_visibility;
import ddgi_types;

// One shared conservative support box per transport sample, NOT receiver rays.
// Near terrain, remove every occupied voxel from the box by its separating face.
// Budget/unknown/outside-domain cases fail closed. Plants are excluded by the
// published terrain occupancy. This can under-water a narrow crevice, never leak.
public VegetationWaterLobe vegetationWaterLobe(float3 center,float3 radius,float3 velocity,float exposure) {
    VegetationWaterLobe lobe=(VegetationWaterLobe)0;
    if(exposure<=0)return lobe;
    U_DdgiVoxelVisibilityInfo info=ddgiPublishedVoxelGeometryInfo();
    if(info.ready==0)return lobe;
    radius=min(radius,float3(8.0/256.0));
    float3 lo=center-radius,hi=center+radius;
    if(ddgiVoxelRegionMayBeOccupied(lo,hi)) {
        float3 scale=info.world_to_voxel_scale;
        int3 first=int3(floor(lo*scale)),last=int3(floor(hi*scale));
        if(any(first<int3(0)) || any(last>=int3(info.voxel_dimensions)) ||
           any(last-first>int3(17)))return lobe;
        for(int z=first.z;z<=last.z;++z)
        for(int y=first.y;y<=last.y;++y)
        for(int x=first.x;x<=last.x;++x) {
            int3 voxel=int3(x,y,z);
            if(!ddgiVoxelIsOccupied(voxel,info.voxel_dimensions))continue;
            float3 a=float3(voxel)/scale,b=float3(voxel+1)/scale;
            float3 separation=max(a-center,center-b);
            uint axis=separation.x>separation.y?0u:1u;
            if(separation.z>separation[axis])axis=2u;
            if(separation[axis]<=0)return lobe;
            if(center[axis]<a[axis])hi[axis]=min(hi[axis],a[axis]-1e-6);
            else lo[axis]=max(lo[axis],b[axis]+1e-6);
        }
    }
    lobe.center_exposure=float4(center,exposure);
    lobe.radius=float4(radius,0);lobe.velocity=float4(velocity,0);
    lobe.lower=float4(lo,0);lobe.upper=float4(hi,0);
    return lobe;
}
