// Shared actual-N view keys for startup baking and runtime selection.
module model_pixel_views;
import model_pixel_projection;
[[vk::binding(17,1)]] StructuredBuffer<float4> model_view_azimuths;
public float3 modelViewDirection(uint index,uint count) {
    float y=1.0-2.0*(float(index)+0.5)/float(count);
    float radius=sqrt(max(0.0,1.0-y*y));
    float2 azimuth=model_view_azimuths[index].xy;
    return float3(azimuth.x*radius,y,azimuth.y*radius);
}
public ModelPixelView canonicalModelView(uint index,uint count) {
    float3 z=modelViewDirection(index,count);
    float3 up=abs(z.y)<0.99?float3(0,1,0):float3(1,0,0);
    float3 x=normalize(cross(up,z)),y=cross(z,x);
    return modelPixelView(float4(x,1),float4(y,0),float4(z,0));
}
public uint nearestModelView(float3 direction,uint count) {
    float best=-2.;uint selected=0;
    for(uint i=0;i<count;i++) {
        float score=dot(direction,modelViewDirection(i,count));
        if(score>best){best=score;selected=i;}
    }
    return selected;
}
