// Flower adapter: prepare path geometry and one final lighting/color per band.
import flower_model;
import flower_stem_geometry;
import flower_stem_experiment;
import stem_band_geometry;
[[vk::binding(16,1)]] RWStructuredBuffer<StemMeshBand> flower_stem_bands;

[shader("compute")]
[numthreads(64,1,1)]
void main(uint3 id:SV_DispatchThreadID) {
    uint plant=id.x;
    if(plant>=flower_pc.prepare_object)return;
    FlowerPlantPose pose=flowerPlantPose(plant);
    FlowerStemFrame frame=(FlowerStemFrame)0;
    frame.root_scale=float4(pose.frame.center-pose.tip_offset-pose.attachment_offset,pose.frame.scale);
    frame.yaw_height_bend=float4(pose.frame.axisX.x,pose.frame.axisX.z,pose.height,pose.bend);
    frame.motion_edge=float4(0,0,0,flowerPart(0u).distribution.y);
    frame.socket=flowerPart(1u).socket;
    if(pose.frame.scale>1e-7)
        frame.motion_edge.xyz=stemToLocalVector(frame,pose.tip_offset)/pose.frame.scale;
    StemGeometry s=stemGeometry(frame);
    s.socket.w=dot(s.socket.xyz,stemCenter(s,0u,1.0));
    float3 socketNormal=normalize(stemToWorldVector(frame,s.socket.xyz));
    float4 socket=float4(socketNormal,dot(socketNormal,stemWorldPoint(frame,stemCenter(s,0u,1.0))));
    uint capacity=flower_pc.padding;
    uint branches=s.branches!=0u?3u:1u;
    float cell=stemWorldCellSize(flowerStemShapeSettings().z,max(pose.frame.scale,1e-7));
    for(uint branch=0u;branch<branches;branch++) {
        float restLength=s.height*(branch==0u?1.0:0.34);
        uint colors=max(1u,uint(ceil(restLength/max(cell,1e-7))));
        uint subdivisions=max(1u,uint(ceil(float(stemSegmentCount(branch))/float(colors))));
        uint segments=colors*subdivisions;
        for(uint colorBand=0u;colorBand<colors;colorBand++) {
            float tm=(float(colorBand)+0.5)/float(colors);
            // Width changes only at material-block boundaries, never at the
            // curve subdivisions inside a block. Preserve base/tip widths.
            bool stepped=flowerStemShapeSettings().y!=0.0;
            float blockT=colors>1u?float(colorBand)/float(colors-1u):0.0;
            float blockRadius=stemRadius(s,branch,blockT);
            float3 sample=stemCenter(s,branch,tm);
            float3 tangent=stemBandDirection(stemCenter(s,branch,min(1.0,tm+0.001))-
                stemCenter(s,branch,max(0.0,tm-0.001)));
            float3 color=pose.frame.scale>1e-7?shadeFlower(flowerStemColor(sample.y,s.height),
                stemWorldPoint(frame,sample),stemToWorldVector(frame,tangent)):float3(0);
            for(uint subdivision=0u;subdivision<subdivisions;subdivision++) {
                uint segment=colorBand*subdivisions+subdivision;
                if(segment>=capacity)continue;
                float t0=float(segment)/float(segments),t1=float(segment+1u)/float(segments);
                StemMeshBand band=(StemMeshBand)0;
                band.a_radius=float4(stemWorldPoint(frame,stemCenter(s,branch,t0)),
                    (stepped?blockRadius:stemRadius(s,branch,t0))*pose.frame.scale);
                band.b_radius=float4(stemWorldPoint(frame,stemCenter(s,branch,t1)),
                    (stepped?blockRadius:stemRadius(s,branch,t1))*pose.frame.scale);
                band.tangent_a=float4(stemToWorldVector(frame,stemBandDirection(
                    stemCenter(s,branch,min(1.0,t0+0.001))-stemCenter(s,branch,max(0.0,t0-0.001)))),0);
                band.tangent_b=float4(stemToWorldVector(frame,stemBandDirection(
                    stemCenter(s,branch,min(1.0,t1+0.001))-stemCenter(s,branch,max(0.0,t1-0.001)))),0);
                band.side=float4(pose.frame.axisX,0);
                band.color_valid=float4(color,pose.frame.scale>1e-7?1.0:0.0);
                band.socket=socket;
                flower_stem_bands[(plant*branches+branch)*capacity+segment]=band;
            }
        }
        for(uint segment=segments;segment<capacity;segment++)
            flower_stem_bands[(plant*branches+branch)*capacity+segment]=(StemMeshBand)0;
    }
}
