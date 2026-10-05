import stone_transform;
import model_pixel_types;
import model_view_quantization;
export __extern_cpp int main()
{
    StonePreviewPush pose;
    pose.translation=float4(1,2,3,0);
    pose.rotation=float4(0,0.7071067811865476,0,0.7071067811865476);
    pose.chunk_origin=float4(-0.25,-0.015625,-0.25,0.5);
    pose.metadata=uint4(0);
    float3 p=stoneWorldPoint(float3(1,0,0),pose);
    float3 back=stoneLocalPoint(p,pose);
    if (length(p-float3(1,2,2))>1e-5) return 1;
    if (length(back-float3(1,0,0))>1e-5) return 2;
    if (length(stoneWorldPoint(float3(0),pose)-pose.translation.xyz)>1e-5) return 3;
    if (length(stoneRotate(float3(0,1,0),pose.rotation)-float3(0,1,0))>1e-5) return 4;
    for (uint i=0;i<64;i++)
    {
        float angle=float(i)*0.17;
        pose.rotation=float4(0,sin(angle*0.5),0,cos(angle*0.5));
        float3 local=float3(float(i)*0.003,0.09,-0.12);
        float3 world=stoneWorldPoint(local,pose);
        if (length(stoneLocalPoint(world,pose)-local)>1e-5) return 5;
        if (abs(length(stoneRotate(local,pose.rotation))-length(local))>1e-5) return 6;
        ModelPixelFrame frame = stonePhysicalFrame(pose);
        if (length(modelViewWorldPoint(frame,local)-world)>1e-5) return 7;
        if (length(modelViewWorldPoint(frame,float3(0))-pose.translation.xyz)>1e-5) return 8;
        if (abs(dot(cross(frame.axisX,frame.axisY),frame.axisZ)-1.0)>1e-5) return 9;
        if (length(modelViewWorldVector(frame,float3(0,1,0))-stoneRotate(float3(0,1,0),pose.rotation))>1e-5) return 10;
    }
    return 0;
}
