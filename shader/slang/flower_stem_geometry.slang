// Camera-independent continuous stem representation and sampling math. No
// scene buffers, history, atlas views, or SDF stepping. A branch is a chain of
// rounded cones (the convex hull of two endpoint spheres), joined by union.
module flower_stem_geometry;

public struct StemGeometry {
    public float height;
    public float bend;
    public float radius;
    public float3 motion;
    public uint branches;
    public float4 socket; // outward normal xyz, dot(normal, live attachment) w
};
public bool stemInsideSocket(StemGeometry s,float3 point) {
    return dot(s.socket.xyz,point)<=s.socket.w+1e-6;
}
public uint stemSegmentCount(uint branch) { return branch==0u?12u:4u; }
public float3 stemCenter(StemGeometry s,uint branch,float t) {
    float h=branch==0u?t:(branch==1u?4.0/12.0:7.0/12.0)+0.23*t;
    float weight=h*h*(3.0-2.0*h);
    float3 p=float3(s.bend*weight,s.height*h,0)+s.motion*weight;
    if(branch!=0u) {
        float3 side=branch==1u?float3(-0.94,0,0.34):float3(0.8,0,-0.6);
        p+=side*(s.height*0.22*t*(2.0-t));
    }
    return p;
}
public float stemRadius(StemGeometry s,uint branch,float t) {
    return s.radius*(branch==0u?lerp(1.0,0.6,t):lerp(0.6,0.18,t));
}
public void stemBounds(StemGeometry s,out float3 lo,out float3 hi) {
    float reach=s.branches!=0u?s.height*0.22:0.0;
    lo=float3(-reach-s.radius,-s.radius,-reach-s.radius)+min(s.motion,float3(0));
    hi=float3(s.bend+reach+s.radius,s.height+s.radius,reach+s.radius)+max(s.motion,float3(0));
}
public struct StemHit {
    public float distance;
    public float3 point;
    public float3 normal;
    public uint branch;
    public float parameter;
};

// Return both real roots, including the linear limiting case of a cone ray.
float2 stemRoots(float a,float b,float c) {
    if(abs(a)<1e-8) {
        float t=abs(b)>1e-8?-c/(2.0*b):-1.0;
        return float2(t,t);
    }
    float d=b*b-a*c;
    if(d<0.0)return float2(-1);
    float q=-b-(b>=0.0?1.0:-1.0)*sqrt(d);
    if(abs(q)<1e-12)return float2(-b/a);
    float2 roots=float2(q/a,c/q);
    return float2(min(roots.x,roots.y),max(roots.x,roots.y));
}
void stemAccept(inout StemHit hit,float distance,float3 ro,float3 rd,
    float3 center,uint branch,float parameter,float4 socket) {
    if(distance>1e-5&&distance<hit.distance&&dot(socket.xyz,ro+rd*distance)<=socket.w+1e-6) {
        hit.distance=distance;hit.point=ro+rd*distance;
        hit.normal=normalize(hit.point-center);
        hit.branch=branch;hit.parameter=parameter;
    }
}
void stemSphereCap(inout StemHit hit,float3 ro,float3 rd,float3 center,
    float radius,float3 axis,float cut,bool upper,uint branch,float parameter,float4 socket) {
    float3 oc=ro-center;
    float b=dot(oc,rd);
    // Closest-approach form avoids subtracting two large squared distances.
    float3 closest=oc-rd*b;
    float d=radius*radius-dot(closest,closest);
    if(d<0.0)return;
    float2 roots=float2(-b-sqrt(d),-b+sqrt(d));
    for(uint i=0u;i<2u;i++) {
        float y=dot(oc+rd*roots[i],axis);
        if(upper?y>=cut:y<=cut)
            stemAccept(hit,roots[i],ro,rd,center,branch,parameter,socket);
    }
}
void stemRoundCone(inout StemHit hit,float3 ro,float3 rd,float3 a,float3 b,
    float ra,float rb,uint branch,float t0,float t1,float4 socket) {
    float3 ab=b-a;
    float h=length(ab);
    if(h<=abs(rb-ra)+1e-6) {
        bool last=rb>ra;
        stemSphereCap(hit,ro,rd,last?b:a,max(ra,rb),float3(0,1,0),
            max(ra,rb),false,branch,last?t1:t0,socket);
        return;
    }
    float3 axis=ab/h;
    float k=(rb-ra)/h;
    float k2=1.0-k*k;
    float3 oc=ro-a;
    float oy=dot(oc,axis),dy=dot(rd,axis);
    float3 ox=oc-axis*oy,dx=rd-axis*dy;
    float r=ra+k*oy;
    float2 roots=stemRoots(k2*dot(dx,dx)-k*k*dy*dy,
        k2*dot(ox,dx)-r*k*dy,k2*dot(ox,ox)-r*r);
    for(uint i=0u;i<2u;i++) {
        float y=oy+roots[i]*dy;
        float centerY=(y+k*ra)/k2;
        if(centerY>=0.0&&centerY<=h) {
            stemAccept(hit,roots[i],ro,rd,a+axis*centerY,branch,
                lerp(t0,t1,centerY/h),socket);
        }
    }
    stemSphereCap(hit,ro,rd,a,ra,axis,-k*ra,false,branch,t0,socket);
    stemSphereCap(hit,ro,rd,b,rb,axis,-k*rb,true,branch,t1,socket);
}
// Membership of the union of endpoint-sphere hulls, for closing the socket
// cut with a real geometric cap instead of hiding/clamping an existing hit.
public bool stemContains(StemGeometry s,float3 point) {
    for(uint branch=0u;branch<(s.branches!=0u?3u:1u);branch++) {
        uint count=stemSegmentCount(branch);
        for(uint segment=0u;segment<count;segment++) {
            float t0=float(segment)/float(count),t1=float(segment+1u)/float(count);
            float3 a=stemCenter(s,branch,t0),b=stemCenter(s,branch,t1),ab=b-a;
            float ra=stemRadius(s,branch,t0),rb=stemRadius(s,branch,t1),dr=rb-ra;
            float denominator=dot(ab,ab)-dr*dr;
            float t=denominator>1e-12?saturate((dot(point-a,ab)+ra*dr)/denominator):(rb>ra?1.0:0.0);
            float3 delta=point-(a+ab*t);float radius=ra+dr*t;
            if(dot(delta,delta)<=radius*radius)return true;
        }
    }
    return false;
}
public StemHit traceStem(StemGeometry s,float3 ro,float3 rd) {
    StemHit hit;hit.distance=1e20;hit.point=float3(0);hit.normal=float3(0,1,0);
    hit.branch=0u;hit.parameter=0.0;
    for(uint branch=0u;branch<(s.branches!=0u?3u:1u);branch++) {
        uint count=stemSegmentCount(branch);
        float3 a=stemCenter(s,branch,0.0);
        float ra=stemRadius(s,branch,0.0);
        for(uint segment=0u;segment<count;segment++) {
            float t0=float(segment)/float(count),t1=float(segment+1u)/float(count);
            float3 b=stemCenter(s,branch,t1);
            float rb=stemRadius(s,branch,t1);
            stemRoundCone(hit,ro,rd,a,b,ra,rb,branch,t0,t1,s.socket);
            a=b;ra=rb;
        }
    }
    float direction=dot(s.socket.xyz,rd);
    if(abs(direction)>1e-8) {
        float distance=(s.socket.w-dot(s.socket.xyz,ro))/direction;
        float3 point=ro+rd*distance;
        if(distance>1e-5&&distance<hit.distance&&stemContains(s,point)) {
            hit.distance=distance;hit.point=point;hit.normal=normalize(s.socket.xyz);
            hit.branch=0u;hit.parameter=1.0;
        }
    }
    return hit;
}

// Quantize material/shading coordinates, NOT visibility/depth. Each axial
// segment shares one center and tangent normal across its entire circumference,
// so both its gradient color and lighting stay uniform, including under wind.
public void stemSurfaceCell(StemGeometry s,StemHit hit,float cellSize,
    out float3 point,out float3 normal) {
    uint branch=hit.branch;
    float restLength=s.height*(branch==0u?1.0:0.34);
    float rings=max(1.0,ceil(restLength/max(cellSize,1e-6)));
    float t=(min(floor(hit.parameter*rings),rings-1.0)+0.5)/rings;
    normal=normalize(stemCenter(s,branch,min(1.0,t+0.001))-
        stemCenter(s,branch,max(0.0,t-0.001)));
    point=stemCenter(s,branch,t);
}
