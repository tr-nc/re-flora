import model_raster_grid;
import flower_stem_model_sampling;
bool close3(float3 a, float3 b) { return all(abs(a-b) < 2e-5); }
export __extern_cpp int main()
{
    ModelRasterView view = {};
    view.center_cell = float4(0,0,10,0.5);
    view.x_near = float4(1,0,0,1);
    view.y_size = float4(0,1,0,8);
    view.normal_distance = float4(0,0,1,10);
    view.cell_origin = float4(-2,-2,0,0);
    float3 camera = float3(0);
    float3 world = float3(0.25,0.25,5);
    float4 clip = modelRasterClip(view,camera,world,0u,64u);
    float2 pixel = (clip.xy/clip.w+1.0)*float(MODEL_RASTER_ATLAS_SIDE)*0.5;
    if(any(abs(pixel-float2(3.5))>2e-4)) return 1;
    if(abs(clip.z/clip.w-0.8)>1e-6) return 2;
    float2 cell = modelRasterSampleCell(view,camera,normalize(world));
    if(any(cell!=float2(3))) return 3;
    for(uint i=0u;i<64u;i++)
    {
        float3 ray=normalize(float3(float(int(i%8u)-4)*0.21,float(int(i/8u)-4)*0.13,1));
        StemModelGrid grid=modelRasterGrid(view);
        float3 sampledRay=stemModelSampleRay(grid,camera,ray);
        float3 hit=sampledRay*(5.0/dot(sampledRay,grid.normal));
        float3 oldDisplay=stemModelDisplayPoint(grid,camera,ray,hit);
        float3 newDisplay=modelRasterDisplayPoint(view,camera,ray,5.0);
        if(!close3(oldDisplay,newDisplay)) return 4;
    }
    // Homogeneous atlas projection remains finite at the eye plane.
    clip=modelRasterClip(view,camera,float3(1,2,0),0u,64u);
    if(any(isnan(clip))||any(isinf(clip))||clip.w!=0.0) return 5;
    uint columns=MODEL_RASTER_ATLAS_SIDE/64u;
    if(any(modelRasterTileOrigin(columns,64u)!=uint2(0,64))) return 6;
    uint last=columns*columns-1u;
    if(any(modelRasterTileOrigin(last,64u)!=uint2(MODEL_RASTER_ATLAS_SIDE-64u))) return 7;
    // Moving one slot never aliases a neighbour's sample locations.
    float4 shifted=modelRasterClip(view,camera,world,1u,64u);
    float2 shiftedPixel=(shifted.xy/shifted.w+1)*float(MODEL_RASTER_ATLAS_SIDE)*0.5;
    if(any(abs(shiftedPixel-pixel-float2(64,0))>2e-4)) return 8;
    return 0;
}
