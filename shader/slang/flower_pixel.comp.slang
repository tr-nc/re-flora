import flower_model;
import flora_types;
import model_pixel_types;
import model_pixel_projection;
import color;
import flora_shadow;
import model_pixel_object;
import model_pixel_cache;
import model_cache_types;
import flower_palette;
[[vk::binding(3,1)]] RWStructuredBuffer<float4> model_pixel_tiles;
[numthreads(8,8,1)]
[shader("compute")]
void main(uint3 id:SV_DispatchThreadID) {
    uint tile=flower_pc.tile_first+id.z;
    ModelPixelFrame frame=flowerPlantFrame(tile/flowerPartCount());
    FlowerModelPart part=flowerTilePart(tile);
    float3 pivot=modelPixelWorld(frame,part.center_radius.xyz);
    float radius=part.center_radius.w*frame.scale;
    if(flower_pc.prepare_object!=0u) {
        if(all(id.xy==uint2(0)))prepareModelObject(tile,pivot,frame,true,part.range.z);
        return;
    }
    uint n=flower_pc.resolution;
    if(any(id.xy>=uint2(n)))return;
    uint outputIndex=(id.z*n+id.y)*n+id.x;
    if(frame.scale<=1e-7) { model_pixel_tiles[outputIndex]=float4(0,0,0,1);return; }
        uint view=uint(model_object_samples[tile*4u+3u].w);
        ModelBakedSurface surface=modelSurface(part.range.z,view,n,id.xy);
        if(surface.positionDepth.w>=1.0) { model_pixel_tiles[outputIndex]=float4(0,0,0,1);return; }
        float3 position=modelPixelWorld(frame,surface.positionDepth.xyz);
        float3 normal=flowerNormal(frame,surface.normalMaterial.xyz);
        if(dot(normal,position-camera_info.pos.xyz)>0.0)normal=-normal;
        float3 linear=flowerPaletteColor(flower_pc.species-MODEL_FLOWER_FIRST_SPECIES,surface.normalMaterial.w);
        float4 light=model_object_samples[tile*4u];
        float3 color=applyStylizedVoxelLightingWithEnvironment(sun_info,linear,
            light.w*directionWeight(normal,sun_info.sun_dir),light.rgb);
        model_pixel_tiles[outputIndex]=float4(color,surface.positionDepth.w);
        return;
}
