// Geometry-free rigid pose shared by cache relighting and ordinary stems.
module model_pixel_types;
public struct ModelPixelFrame {
    public float3 center;
    public float3 axisX;
    public float3 axisY;
    public float3 axisZ;
    public float scale;
};
public ModelPixelFrame identityModelPixelFrame() {
    ModelPixelFrame f;
    f.center=float3(0.0); f.axisX=float3(1,0,0);
    f.axisY=float3(0,1,0); f.axisZ=float3(0,0,1); f.scale=1.0;
    return f;
}
public float3 modelPixelWorld(ModelPixelFrame f,float3 p) {
    // Coverage lives on exact cell boundaries. Keep projection arithmetic
    // stable when unrelated material branches change compiler contraction.
    precise float3 world=f.center+f.scale*(f.axisX*p.x+f.axisY*p.y+f.axisZ*p.z);
    return world;
}
public float3 modelPixelLocalVector(ModelPixelFrame f,float3 v) {
    return float3(dot(v,f.axisX),dot(v,f.axisY),dot(v,f.axisZ));
}
