import flower_stem_geometry;

bool close(float a,float b) { return abs(a-b)<2e-5; }
export __extern_cpp int main() {
    StemGeometry s;
    s.height=2.0;s.bend=0.0;s.radius=0.1;s.motion=float3(0);s.branches=0u;
    StemHit side=traceStem(s,float3(0,1,1),float3(0,0,-1));
    // Independent closed-form middle of a linear tapered hull of spheres.
    if(!close(side.distance,1.0-0.08/sqrt(1.0-0.02*0.02)))return 1;
    if(!close(side.normal.y,0.02)||!close(length(side.normal),1.0))return 2;
    if(!close(traceStem(s,float3(0,-1,0),float3(0,1,0)).distance,0.9))return 3;
    if(!close(traceStem(s,float3(0,3,0),float3(0,-1,0)).distance,0.94))return 4;
    if(traceStem(s,float3(1,1,1),float3(0,0,-1)).distance<1e19)return 5;
    // The axial limiting case must remain finite for short/fat plants, too.
    s.height=0.01;s.radius=0.1;
    if(!close(traceStem(s,float3(0,-1,0),float3(0,1,0)).distance,0.9))return 6;

    // Real production bounds contain every swept endpoint ball, with taper,
    // forks and the maximum permitted wind displacement (both signs).
    for(uint shape=0u;shape<8u;shape++) {
        s.height=shape<4u?0.02:8.0;s.radius=shape%2u==0u?0.005:0.2;
        s.bend=0.08;s.branches=1u;
        s.motion=float3(shape%2u==0u?-0.2:0.2,0,shape%3u==0u?-0.15:0.15)*s.height;
        float3 lo,hi;stemBounds(s,lo,hi);
        for(uint branch=0u;branch<3u;branch++) {
            uint count=stemSegmentCount(branch);
            for(uint i=0u;i<=count;i++) {
                float t=float(i)/float(count),r=stemRadius(s,branch,t);
                float3 p=stemCenter(s,branch,t);
                if(any(p-r<lo-1e-6)||any(p+r>hi+1e-6))return 7;
            }
        }
        if(length(stemCenter(s,1u,0)-stemCenter(s,0u,4.0/12.0))>1e-6)return 8;
        if(length(stemCenter(s,2u,0)-stemCenter(s,0u,7.0/12.0))>1e-6)return 9;
        float3 tip=stemCenter(s,0u,1);
        if(length(tip-(float3(s.bend,s.height,0)+s.motion))>1e-6)return 10;
    }

    // World-axis cubemap cells: finite, idempotent, six faces, deterministic
    // seams. The angular proxy/culling margin must dominate actual ray error.
    for(uint n=128u;n<=2048u;n*=2u) {
        for(uint axis=0u;axis<3u;axis++)for(int sign=-1;sign<=1;sign+=2) {
            for(int i=-16;i<=16;i++)for(int j=-16;j<=16;j++) {
                float3 ray=float3(0);ray[axis]=float(sign);
                ray[(axis+1u)%3u]=float(i)/16.0;ray[(axis+2u)%3u]=float(j)/16.0;
                ray=normalize(ray);
                float3 cell=stemDirectionCell(ray,n);
                if(!close(length(cell),1.0)||length(cell-ray)>4.0/float(n))return 11;
                if(length(stemDirectionCell(cell,n)-cell)>1e-6)return 12;
            }
        }
        // Two nearby display rays sample the same source regardless of where
        // a camera rotation would place those rays on the screen.
        float3 a=stemDirectionCell(normalize(float3(0.00001,0.00001,1)),n);
        float3 b=stemDirectionCell(normalize(float3(0.00002,0.00002,1)),n);
        if(any(a!=b))return 13;
    }
    s.height=2;s.bend=0;s.radius=0.1;s.motion=float3(0);s.branches=0u;
    float3 point,normal;
    StemHit interior=side;interior.parameter=0.523;
    stemSurfaceCell(s,interior,0.1,point,normal);
    if(!close(length(normal),1.0)||any(isnan(point))||any(isnan(normal)))return 14;
    StemHit neighbour=interior;neighbour.parameter+=0.001;
    float3 point2,normal2;stemSurfaceCell(s,neighbour,0.1,point2,normal2);
    if(length(point-point2)>1e-6||length(normal-normal2)>1e-6)return 15;
    return 0;
}
