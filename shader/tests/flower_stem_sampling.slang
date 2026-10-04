import flower_stem_geometry;

bool close(float a,float b) { return abs(a-b)<2e-5; }
export __extern_cpp int main() {
    StemGeometry s;
    s.height=2.0;s.bend=0.0;s.radius=0.1;s.motion=float3(0);s.branches=0u;s.socket=float4(0);
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

    s.height=2;s.bend=0;s.radius=0.1;s.motion=float3(0);s.branches=0u;
    float3 point,normal;
    StemHit interior=side;interior.parameter=0.523;
    stemSurfaceCell(s,interior,0.1,point,normal);
    if(!close(length(normal),1.0)||any(isnan(point))||any(isnan(normal)))return 14;
    StemHit neighbour=interior;neighbour.parameter+=0.001;
    float3 point2,normal2;stemSurfaceCell(s,neighbour,0.1,point2,normal2);
    if(length(point-point2)>1e-6||length(normal-normal2)>1e-6)return 15;
    // All sides of a ring must use identical material AND lighting inputs.
    // Exercise bent, wind-displaced stems and both forks, not just a vertical stem.
    s.bend=0.08;s.motion=float3(0.2,0,-0.15);s.branches=1u;
    for(uint branch=0u;branch<3u;branch++) {
        interior.branch=branch;
        stemSurfaceCell(s,interior,0.1,point,normal);
        for(uint face=0u;face<4u;face++) {
            neighbour=interior;
            float angle=float(face)*1.57079632679;
            neighbour.normal=float3(cos(angle),0,sin(angle));
            neighbour.point+=neighbour.normal*s.radius;
            stemSurfaceCell(s,neighbour,0.1,point2,normal2);
            if(length(point-point2)>1e-6||length(normal-normal2)>1e-6)return 16;
        }
        neighbour.parameter=0.8;
        stemSurfaceCell(s,neighbour,0.1,point2,normal2);
        if(length(point-point2)<0.01)return 17;
    }
    // Fixed world-height blocks shrink in count with growth/size, not in
    // physical height. A two-unit stem has 20, 10 and 1 rings at these scales.
    s.height=2;s.bend=0;s.motion=float3(0);
    interior.branch=0u;interior.parameter=0.01;
    for(uint i=0u;i<3u;i++) {
        float scale=i==0u?1.0:(i==1u?0.5:0.01);
        float cell=stemWorldCellSize(0.1,scale);
        stemSurfaceCell(s,interior,cell,point,normal);
        float expected=i<2u?0.05:0.01;
        if(!close(point.y*scale,expected))return 18;
        if(any(isnan(point))||any(isnan(normal)))return 19;
    }
    // The original mode still uses model-space layers regardless of scale.
    stemSurfaceCell(s,interior,0.1,point,normal);
    if(!close(point.y*0.5,0.025))return 20;
    if(!close(stemWorldCellSize(0.1,0.0),1000000.0))return 21;
    return 0;
}
