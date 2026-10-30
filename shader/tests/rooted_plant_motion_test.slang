import rooted_plant_motion;
export __extern_cpp int main() {
    float3 tip=float3(.015,0,-.008),up=float3(0,1,0);
    for(uint i=0u;i<4u;i++) {
        float scale=.01+float(i)*.025,height=2.3;
        if(length(rootedPlantOffset(0,height,tip))>1e-9)return 1;
        if(length(rootedPlantOffset(height,height,tip)-tip)>1e-7)return 2;
        for(uint j=1u;j<10u;j++) {
            float y=height*float(j)/10.0,epsilon=.001;
            float3 tangent=up+(rootedPlantOffset(y+epsilon,height,tip)-
                rootedPlantOffset(y-epsilon,height,tip))/(2*epsilon*scale);
            // A vertical branch's X/Z normals stay perpendicular to its bent tangent.
            float3 nx=rootedPlantNormal(float3(1,0,0),up,y,height,scale,tip);
            float3 nz=rootedPlantNormal(float3(0,0,1),up,y,height,scale,tip);
            if(abs(dot(nx,normalize(tangent)))>2e-4||abs(dot(nz,normalize(tangent)))>2e-4)return 3;
            float3 offset=rootedPlantOffset(y,height,tip);
            if(length(offset)>length(tip)+1e-7)return 4;
        }
    }
    if(length(rootedPlantOffset(-1,2,tip))>0)return 5;
    return 0;
}
