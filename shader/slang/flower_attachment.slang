// Current-frame attachment, shared by native/cached butterfly geometry.
// CPU contacts select intent; attachment itself never follows delayed readback.
module flower_attachment;
import flower_visit_types;
import model_pixel_types;
// Graphics set 1 slots 20..27 belong to the four model surface caches.
// Compute contact output uses its own pipeline layout; graphics must not alias it.
[[vk::binding(28,1)]] public StructuredBuffer<FlowerVisitPose> flower_visit_output;
public bool attachFlower(uint4 attachment,float scale,inout ModelPixelFrame frame) {
    if(attachment.y==0u)return true;
    FlowerVisitPose pose=flower_visit_output[attachment.x];
    if(pose.tangent_valid.w<0.5)return false;
    float yaw=asfloat(attachment.z);
    frame.axisY=pose.normal_growth.xyz;
    frame.axisZ=normalize(pose.tangent_valid.xyz*cos(yaw)+cross(frame.axisY,pose.tangent_valid.xyz)*sin(yaw));
    frame.axisX=normalize(cross(frame.axisY,frame.axisZ));
    frame.center=pose.position_radius.xyz+frame.axisY*(asfloat(attachment.w)*scale);
    return true;
}
