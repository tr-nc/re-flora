module apple_pixel_dynamic_pose;
import apple_pixel_frame;
public ApplePixelVertex appleDynamicPose(float3 center,float4 rotation,float scale,uint resolution,uint index) {
    ApplePixelVertex pose;pose.center=center;
    float3 x=float3(1,0,0),y=float3(0,1,0),z=float3(0,0,1);
    pose.axisX=x+2.0*cross(rotation.xyz,cross(rotation.xyz,x)+rotation.w*x);
    pose.axisY=y+2.0*cross(rotation.xyz,cross(rotation.xyz,y)+rotation.w*y);
    pose.axisZ=z+2.0*cross(rotation.xyz,cross(rotation.xyz,z)+rotation.w*z);
    pose.scale=(2.0/256.0)*scale;pose.resolution=resolution;pose.tileIndex=index;
    return pose;
}
