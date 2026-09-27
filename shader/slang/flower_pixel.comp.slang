import flower_model;
import model_pixel_surface;
import model_pixel_projection;
import model_pixel_grid;
import butterfly_mesh_types;
import color;
import flora_shadow;
import model_pixel_object;
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
        if(all(id.xy==uint2(0)))prepareModelObject(tile,pivot,frame,true);
        return;
    }
    uint n=flower_pc.resolution;
    if(any(id.xy>=uint2(n)))return;
    uint outputIndex=(id.z*n+id.y)*n+id.x;
    if(frame.scale<=1e-7) { model_pixel_tiles[outputIndex]=float4(0,0,0,1);return; }
    ModelPixelHit hit;
    if(modelDiscreteViews())hit=sampleOrthographicModelPixel(flower_triangles,part.range.x,part.range.y,
        frame,modelObjectView(tile),pivot,radius,id.xy,n);
    else hit=sampleModelPixelGeometry(flower_triangles,part.range.x,part.range.y,frame,
        modelPixelBounds(pivot,radius,camera_info),id.xy,n,camera_info);
    if(hit.triangle==0xffffffffu) { model_pixel_tiles[outputIndex]=float4(0,0,0,1);return; }
    ButterflyMeshTriangle tri=flower_triangles[hit.triangle];
    if(modelDiscreteViews()) {
        hit.position=modelPixelWorld(frame,tri.a.xyz+tri.e1.xyz*hit.bary.x+tri.e2.xyz*hit.bary.y);
        hit.direction=normalize(hit.position-camera_info.pos.xyz);
    }
    float3 normal=flowerNormal(frame,tri.normal0.xyz);
    if(dot(normal,hit.direction)>0.0)normal=-normal;
    float3 color;
    if(modelSingleLight()) {
        float4 light=model_object_samples[tile*4u];
        color=applyStylizedVoxelLightingWithEnvironment(sun_info,srgbToLinear(tri.uv01.xyz),
            light.w*directionWeight(normal,sun_info.sun_dir),light.rgb);
    }else color=shadeFlower(tri.uv01.xyz,hit.position,normal);
    model_pixel_tiles[outputIndex]=float4(color,hit.depth);
}
