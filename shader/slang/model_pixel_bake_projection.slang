// Canonical geometry projection, never imported by frame consumers.
module model_pixel_bake_projection;
import model_pixel_bake_geometry;
import model_pixel_types;
import model_bake_triangle;
import tracer_types;
import model_pixel_projection;
public U_CameraInfo modelBakeCamera() {
    U_CameraInfo camera=(U_CameraInfo)0;
    // Rays start at z=+1 and travel toward -Z. Identical for every instance.
    camera.view_proj_mat=float4x4(1,0,0,0, 0,1,0,0, 0,0,-0.5,0.5, 0,0,0,1);
    camera.view_proj_mat_inv=float4x4(1,0,0,0, 0,1,0,0, 0,0,-2,1, 0,0,0,1);
    return camera;
}
public ModelPixelFrame modelBakeFrame(ModelPixelFrame physical,ModelPixelView view,float3 pivot,float radius) {
    ModelPixelFrame bake;
    bake.center=modelBakeVector(view,physical.center-pivot)/radius;
    bake.axisX=modelBakeVector(view,physical.axisX);
    bake.axisY=modelBakeVector(view,physical.axisY);
    bake.axisZ=modelBakeVector(view,physical.axisZ);
    bake.scale=physical.scale/radius;
    return bake;
}
public ModelPixelHit sampleOrthographicModelPixel<T : IModelPixelProjectionObserver>(StructuredBuffer<ButterflyMeshTriangle> triangles,
    uint first,uint count,ModelPixelFrame physical,ModelPixelView view,float3 pivot,float radius,
    uint2 pixel,uint resolution,T observer) {
    if(radius<=1e-8) {
        ModelPixelHit empty=(ModelPixelHit)0;empty.triangle=0xffffffffu;empty.depth=1.0;return empty;
    }
    ModelPixelFrame bake=modelBakeFrame(physical,view,pivot,radius);
    return sampleModelPixelGeometry(triangles,first,count,bake,float4(-1,-1,1,1),pixel,resolution,modelBakeCamera(),observer);
}
