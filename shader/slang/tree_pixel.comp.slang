// Live mesh adapter: exact current camera, current GPU surface and refit BVH.
// No rigid model cache, discrete views, whole-tree lighting sample or voxel data.
import tree_pixel_pass;
import tree_scene;
import tree_scene_types;
import model_pixel_surface;
import model_pixel_grid;
import butterfly_mesh_types;
import tree_display_material;
import tracer_types;
[[vk::binding(3,0)]] ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(11,0)]] ConstantBuffer<U_VoxelColors> voxel_colors;
[[vk::binding(48,0)]] StructuredBuffer<float4> raster_tree_light_cache;
[[vk::binding(0,1)]] RWStructuredBuffer<float4> tree_pixel_tiles;
[[vk::push_constant]] TreePixelPass pass;

bool treePixelNodeIntersects(TreeSceneNode node,float4 planes[6]) {
    for(uint i=0u;i<6u;i++) {
        float3 support=lerp(node.lower,node.upper,step(float3(0),planes[i].xyz));
        // Broad phase only; the common clipped triangle/cell test is authoritative.
        if(dot(planes[i].xyz,support)+planes[i].w < -1e-6)return false;
    }
    return true;
}

[shader("compute")]
[numthreads(8,8,1)]
void main(uint3 id:SV_DispatchThreadID) {
    uint2 grid=treePixelGrid(pass);
    if(id.x>=grid.x || id.y>=pass.band.y)return;
    uint2 pixel=id.xy+uint2(0,pass.band.x);
    float4 bounds=treePixelBounds(pass);
    float3 origin,direction;
    modelPixelRay(bounds,float2(pixel)+0.5,grid.x,camera_info,origin,direction);
    TreeTriangleHit center=closestTreeTriangle(origin,direction,1e10,true);
    ModelPixelHit hit=(ModelPixelHit)0;
    hit.triangle=0xffffffffu;hit.depth=1.0;
    if(center.primitive!=0xffffffffu) {
        float4 clip=mul(camera_info.view_proj_mat,float4(origin+direction*center.distance,1));
        float depth=clip.z/clip.w;
        if(depth>=0.0 && depth<1.0) {
            hit.triangle=center.primitive;hit.bary=center.bary;hit.depth=depth;
        }
    }
    // Original center hits win, exactly as in the shared model display. Only
    // empty centers need conservative cell coverage (including hairline twigs).
    if(hit.triangle==0xffffffffu) {
        float2 lo=modelPixelNdc(bounds,float2(pixel)-0.0001,grid.x);
        float2 hi=modelPixelNdc(bounds,float2(pixel)+1.0001,grid.x);
        float4x4 m=camera_info.view_proj_mat;
        float4 planes[6];
        planes[0]=m[0]-lo.x*m[3];planes[1]=hi.x*m[3]-m[0];
        planes[2]=m[1]-lo.y*m[3];planes[3]=hi.y*m[3]-m[1];
        planes[4]=m[2];planes[5]=m[3]-m[2];
        uint index=0u,count=treeSceneNodeCount();
        while(index<count) {
            TreeSceneNode node=treeSceneNode(index);
            if(!treePixelNodeIntersects(node,planes)){index=node.escape;continue;}
            if(node.primitive==0xffffffffu){++index;continue;}
            uint3 ids=treeScenePrimitive(node.primitive).xyz;
            float3 a=treeScenePosition(ids.x),b=treeScenePosition(ids.y),c=treeScenePosition(ids.z);
            float3 e1=b-a,e2=c-a;
            // Match the ordinary wood draw's back-face culling, including when
            // the camera is inside a trunk. Queries remain deliberately two-sided.
            if(dot(cross(e1,e2),camera_info.pos.xyz-a)>0.0) {
                ButterflyMeshTriangle tri=(ButterflyMeshTriangle)0;
                tri.a=float4(a,0);tri.e1=float4(e1,0);tri.e2=float4(e2,0);
                // Continuous surface sampling, not the rigid-model centroid rule.
                hit=modelPixelTriangleCoverageHit(tri,node.primitive,identityModelPixelFrame(),
                    bounds,pixel,grid.x,camera_info,hit,true);
            }
            index=node.escape;
        }
    }
    float4 sample=float4(0,0,0,1);
    if(hit.triangle!=0xffffffffu) {
        uint3 ids=treeScenePrimitive(hit.triangle).xyz;
        float3 light=raster_tree_light_cache[ids.x].xyz*(1.0-hit.bary.x-hit.bary.y)
            +raster_tree_light_cache[ids.y].xyz*hit.bary.x+raster_tree_light_cache[ids.z].xyz*hit.bary.y;
        sample=float4(shadeTreeWood(light,voxel_colors),hit.depth);
    }
    tree_pixel_tiles[id.y*grid.x+id.x]=sample;
}
