import contree_node;
import local_lighting;
import local_emitter;
import local_visibility_policy;
import voxel_data;
import voxel_material;

[[vk::binding(42,0)]] ConstantBuffer<U_LocalLightInfo> local_light_info;
[[vk::binding(43,0)]] StructuredBuffer<U_LightGpu> local_lights;
[[vk::binding(44,0)]] StructuredBuffer<ContreeNode> contree_node_data;
[[vk::binding(45,0)]] StructuredBuffer<uint> contree_leaf_data;
[[vk::binding(46,0)]] [[vk::image_format("rg32ui")]] readonly RWTexture3D<uint2> scene_tex;
[[vk::binding(0,1)]] RWStructuredBuffer<float4> local_light_field;

struct LocalLightFieldPushConstants { uint first_cell; uint total_cells; uint light_count; uint geometry_revision; uint ready; };
[[vk::push_constant]] ConstantBuffer<LocalLightFieldPushConstants> pc;

float4 buildCertificate(U_LightGpu light, uint sourceId, uint cell)
{
    float4 certificate=float4(-1.0,-1.0,0.0,0.0);
    if (light.abi_version!=LOCAL_LIGHT_GPU_ABI_VERSION || light.kind!=LOCAL_LIGHT_KIND_POINT ||
        light.shape_params.w<=0.5 || any(light.shape_params.xyz<=float3(0.0))) return certificate;
    LocalEmitterSample source=localEmitterSource(light.position,light.shape_params.xyz,sourceId);
    uint face; float2 lo,hi;
    visibilityCellSlopes(cell,face,lo,hi);
    // Only an entirely outward cone may omit the authored emitter interior.
    float outward=min(min(dot(source.normal,visibilityFaceVector(face,lo)),
        dot(source.normal,visibilityFaceVector(face,hi))),
        min(dot(source.normal,visibilityFaceVector(face,float2(lo.x,hi.y))),
            dot(source.normal,visibilityFaceVector(face,float2(hi.x,lo.y)))));
    if (outward<0.0) return certificate;
    uint w,h,d; scene_tex.GetDimensions(w,h,d);
    if (w*h*d>4096u) return certificate;
    float clearDepth=light.range+0.1;
    float3 blocker=float3(-1.0,0.0,0.0);
    uint visits=0u;
    // Only four levels in a 256^3 published chunk. No scratch shared with the
    // ray marcher, and no editable/ahead-of-publication atlas reads.
    uint nodes[5], cursors[5]; float3 bases[5]; float sizes[5];
    for (uint z=0u;z<d;++z) for(uint y=0u;y<h;++y) for(uint x=0u;x<w;++x)
    {
        float3 base=float3(x,y,z); float nearDepth;
        if (!visibilityConeMayIntersect(source.position,face,lo,hi,base,base+1.0,clearDepth,nearDepth)) continue;
        uint2 chunk=scene_tex[uint3(x,y,z)];
        if(chunk.x==0u) continue;
        int level=0;
        nodes[0]=0u; cursors[0]=0u; bases[0]=base; sizes[0]=1.0;
        while(level>=0)
        {
            if(++visits>32768u) return float4(-1.0,-1.0,0.0,0.0);
            uint child=cursors[level]++;
            if(child>=64u) { --level; continue; }
            ContreeNode node=contree_node_data[chunk.x-1u+nodes[level]];
            if(!childMaskTest(node,child)) continue;
            float size=sizes[level]*0.25;
            float3 lower=bases[level]+float3(child&3u,child>>4u,(child>>2u)&3u)*size;
            float3 upper=lower+size;
            if(!visibilityConeMayIntersect(source.position,face,lo,hi,lower,upper,clearDepth,nearDepth)) continue;
            uint index=(node.packed_0>>1u)+childMaskBitCountBelow(node,child);
            if((node.packed_0 & 1u)!=0u)
            {
                uint type=voxelTypeFromData(contree_leaf_data[chunk.y-1u+index]);
                // Match the authoritative endpoint owner. Never omit foreign
                // edits or a whole internal node merely inside source bounds.
                if(abs(size-1.0/256.0)<1.0e-7 &&
                    finiteEmitterOwnsHit(light,type,(lower+upper)*0.5)) continue;
                clearDepth=min(clearDepth,nearDepth);
                // Wood can be a live raster-tree rest cell. Never certify it
                // as a static blocker; live tree geometry is queried at receivers.
                VoxelMaterial material=voxelMaterial(type,(local_light_info.flags & LOCAL_LIGHT_FLAG_LEGACY_SAND_GLASS)!=0u);
                if(type!=5u && material.local_shadow==VOXEL_LOCAL_SHADOW_OPAQUE && abs(size-1.0/256.0)<1.0e-7)
                    blocker=(lower+upper)*0.5;
            }
            else
            {
                if(level>=3) return float4(-1.0,-1.0,0.0,0.0);
                ++level;
                nodes[level]=index; cursors[level]=0u; bases[level]=lower; sizes[level]=size;
            }
        }
    }
    return float4(max(0.0,clearDepth-LOCAL_VISIBILITY_EPSILON),blocker);
}

[shader("compute")]
[numthreads(64,1,1)]
void main(uint3 id:SV_DispatchThreadID)
{
    uint cell=pc.first_cell+id.x;
    if(cell>=pc.total_cells) return;
    if(id.x==0u)
    {
        local_light_field[0]=asfloat(uint4(LOCAL_VISIBILITY_ABI,pc.light_count,
            local_light_info.live_revision_low,local_light_info.live_revision_high));
        local_light_field[1]=asfloat(uint4(pc.geometry_revision,LOCAL_VISIBILITY_SIDE,pc.ready,0u));
    }
    uint lightIndex=cell/(LOCAL_VISIBILITY_CELLS*LOCAL_VISIBILITY_SOURCES);
    if(lightIndex>=pc.light_count) return;
    uint sourceId=(cell/LOCAL_VISIBILITY_CELLS)%LOCAL_VISIBILITY_SOURCES;
    local_light_field[cell+2u]=buildCertificate(local_lights[lightIndex],sourceId,cell%LOCAL_VISIBILITY_CELLS);
}
