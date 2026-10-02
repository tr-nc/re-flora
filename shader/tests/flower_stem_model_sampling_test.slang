import flower_stem_geometry;
import flower_stem_model_sampling;

uint modelCells(float distance) {
    float3 camera=float3(0,0,distance);
    StemModelGrid grid=stemModelGrid(float3(0),1.1,2.0,128u,camera,float3(1,0,0));
    uint count=0u;float3 previous=float3(0);
    for(uint i=0u;i<=256u;i++) {
        float3 ray=normalize(float3(float(i)/256.0-0.5,0,-distance));
        float3 sampled=stemModelSampleRay(grid,camera,ray);
        if(i==0u||length(sampled-previous)>1e-7)count++;
        previous=sampled;
    }
    return count;
}
export __extern_cpp int main() {
    // Keep exactly the same source samples for the same model-unit span.
    if(modelCells(2.0)!=65u||modelCells(8.0)!=65u||modelCells(32.0)!=65u)return 2;
    for(uint i=0u;i<100u;i++) {
        float x=float(i)/99.0-0.5;
        float3 nearCamera=float3(0,0,2),farCamera=float3(0,0,8);
        StemModelGrid a=stemModelGrid(float3(0),1.1,2.0,128u,nearCamera,float3(1,0,0));
        StemModelGrid b=stemModelGrid(float3(0),1.1,2.0,128u,farCamera,float3(1,0,0));
        float3 ra=stemModelSampleRay(a,nearCamera,normalize(float3(x,0,-2)));
        float3 rb=stemModelSampleRay(b,farCamera,normalize(float3(x,0,-8)));
        float3 pa=nearCamera+ra*(-2.0/ra.z),pb=farCamera+rb*(-8.0/rb.z);
        if(length(pa-pb)>1e-5||abs(a.cell-b.cell)>1e-7)return 3;
    }
    // Continuous orbit (including world-axis ties and a complete revolution):
    // no nearest canonical view switches in the lattice or its perspective ray.
    float3 lastX=float3(0),lastRay=float3(0);
    for(uint i=0u;i<=1440u;i++) {
        float angle=float(i)*6.28318530718/1440.0;
        float3 camera=float3(sin(angle)*3,0.7,cos(angle)*3);
        float3 right=float3(cos(angle),0,-sin(angle));
        StemModelGrid grid=stemModelGrid(float3(0),1.1,2.0,128u,camera,right);
        float3 display=normalize(grid.center+grid.x*0.153+grid.y*0.081-camera);
        float3 sampled=stemModelSampleRay(grid,camera,display);
        if(i>0u&&(length(grid.x-lastX)>0.005||length(sampled-lastRay)>0.005))return 4;
        lastX=grid.x;lastRay=sampled;
    }
    // Culling fringe and depth for near, far, and inside-bound cameras. The
    // production hit comes from the same analytic cones as the fragment shader.
    StemGeometry s;s.height=2;s.bend=0;s.radius=0.15;s.motion=float3(0);s.branches=1u;s.socket=float4(0);
    uint hits=0u;
    for(uint d=0u;d<4u;d++) {
        float distance=d==0u?0.02:(d==1u?0.5:(d==2u?2.0:8.0));
        float3 camera=float3(0,1,distance);
        StemModelGrid grid=stemModelGrid(float3(0,1,0),1.2,s.height,128u,camera,float3(1,0,0));
        for(int i=-30;i<=30;i++) {
            float3 ray=normalize(float3(float(i)*0.003,0,-distance));
            float3 sample=stemModelSampleRay(grid,camera,ray);
            StemHit hit=traceStem(s,camera,sample);
            if(hit.distance>=1e19)continue;
            hits++;
            float3 displayed=stemModelDisplayPoint(grid,camera,ray,hit.point);
            if(any(isnan(displayed))||any(isinf(displayed)))return 5;
            if(length(displayed-hit.point)>2.0*s.height/128.0+1e-5)return 6;
            if(abs(dot(displayed-hit.point,grid.normal))>1e-5)return 7;
            // Surface-cell shading must not change this sampling/depth contract.
            float3 point,normal;stemSurfaceCell(s,hit,0.05,point,normal);
            if(any(isnan(point))||any(isnan(normal)))return 8;
        }
    }
    if(hits==0u)return 9;
    StemModelGrid singular=stemModelGrid(float3(0),1,2,128u,float3(0),float3(1,0,0));
    if(any(stemModelSampleRay(singular,float3(0),float3(0,0,-1))!=float3(0,0,-1)))return 10;
    return 0;
}
