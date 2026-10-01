// Actual branch-attached voxel slabs, not quantized shading or fewer cones.
// The authored grid is fixed. Each Y slab receives the same affine wind warp
// on all branches, so shared faces stay joined and cell identity never refits.
module flower_stem_blocks;
import flower_stem_geometry;
public struct StemBlockInterval {
    public float lo;public float hi;public float3 enterNormal;public float3 exitNormal;
    public uint branch;public float parameter;
};
float stemBlockWeight(float y,float height) {
    float t=saturate(y/max(height,1e-6));return t*t*(3.0-2.0*t);
}
public StemBlockInterval stemBlockInterval(StemGeometry s,float3 ro,float3 rd,
    uint row,uint branch,float cell) {
    StemBlockInterval result;
    result.lo=1e20;result.hi=-1e20;result.enterNormal=result.exitNormal=float3(0,1,0);
    result.branch=branch;result.parameter=0;
    float y0=float(row)*cell,y1=float(row+1u)*cell;
    float start=branch==0u?0.0:(branch==1u?4.0:7.0)*s.height/12.0;
    float length=s.height*(branch==0u?1.0:0.23);
    if(y1<=start||y0>=start+length)return result;
    float t0=saturate((y0-start)/max(length,1e-6));
    float t1=saturate((y1-start)/max(length,1e-6));
    result.parameter=(t0+t1)*.5;
    StemGeometry rest=s;rest.motion=float3(0);
    float3 a=stemCenter(rest,branch,t0),b=stemCenter(rest,branch,t1);
    float3 mid=stemCenter(rest,branch,result.parameter);
    // Odd-cell square cross section, with one explicit voxel minimum. Coarse
    // geometry changes thickness as well as silhouette; it is not antialiasing.
    float halfWidth=(floor(stemRadius(s,branch,t0)/cell)+.5)*cell;
    float3 low=min(min(floor(a/cell+.5),floor(b/cell+.5)),floor(mid/cell+.5))*cell-halfWidth;
    float3 high=max(max(floor(a/cell+.5),floor(b/cell+.5)),floor(mid/cell+.5))*cell+halfWidth;
    low.y=y0;high.y=y1;
    float w0=stemBlockWeight(y0,s.height),w1=stemBlockWeight(y1,s.height);
    float3 slope=s.motion*((w1-w0)/cell);
    // Production wind is bounded by height/4, hence 1+slope.y >= .625.
    // Inverse affine warp leaves ray distance in the original local units.
    float denom=max(1.0+slope.y,1e-4);
    float restY=y0+(ro.y-y0-s.motion.y*w0)/denom;
    float3 origin=ro-s.motion*w0-slope*(restY-y0);origin.y=restY;
    float3 direction=rd-slope*(rd.y/denom);direction.y=rd.y/denom;
    float lo=-1e20,hi=1e20;float3 nearNormal=float3(0),farNormal=float3(0);
    for(uint axis=0u;axis<3u;axis++) {
        if(abs(direction[axis])<1e-8) {
            if(origin[axis]<low[axis]||origin[axis]>high[axis])return result;
        } else {
            float aT=(low[axis]-origin[axis])/direction[axis];
            float bT=(high[axis]-origin[axis])/direction[axis];
            float3 n=float3(0);n[axis]=direction[axis]>0.0?-1.0:1.0;
            if(min(aT,bT)>lo){lo=min(aT,bT);nearNormal=n;}
            if(max(aT,bT)<hi){hi=max(aT,bT);farNormal=-n;}
        }
    }
    // A^-T maps true face normals, including the slanted wind-warped faces.
    nearNormal.y=(nearNormal.y-dot(slope.xz,nearNormal.xz))/denom;
    farNormal.y=(farNormal.y-dot(slope.xz,farNormal.xz))/denom;
    nearNormal=normalize(nearNormal);farNormal=normalize(farNormal);
    float planeDirection=dot(s.socket.xyz,rd);
    if(abs(planeDirection)<1e-8) {
        if(!stemInsideSocket(s,ro))return result;
    } else {
        float planeT=(s.socket.w-dot(s.socket.xyz,ro))/planeDirection;
        if(planeDirection<0.0&&planeT>lo){lo=planeT;nearNormal=normalize(s.socket.xyz);}
        if(planeDirection>0.0&&planeT<hi){hi=planeT;farNormal=normalize(s.socket.xyz);}
    }
    if(hi<lo||hi<=1e-5)return result;
    result.lo=lo;result.hi=hi;result.enterNormal=nearNormal;result.exitNormal=farNormal;
    return result;
}
bool stemBlockConsume(StemBlockInterval interval,float3 ro,float3 rd,
    inout bool inside,inout float connectedExit,inout StemHit hit) {
    if(interval.hi<interval.lo||interval.hi<=1e-5)return false;
    if(!inside&&interval.lo>1e-5) {
        hit.distance=interval.lo;hit.normal=interval.enterNormal;
        hit.branch=interval.branch;hit.parameter=interval.parameter;
        hit.point=ro+rd*hit.distance;return true;
    }
    if(inside&&interval.lo>connectedExit+1e-5) {
        hit.point=ro+rd*hit.distance;return true;
    }
    inside=true;
    if(interval.hi>connectedExit) {
        connectedExit=interval.hi;hit.distance=interval.hi;hit.normal=interval.exitNormal;
        hit.branch=interval.branch;hit.parameter=interval.parameter;
    }
    return false;
}
public StemHit traceStemBlocks(StemGeometry s,float3 ro,float3 rd,float cellSize) {
    float cell=max(cellSize,1e-5);
    uint rows=max(1u,uint(ceil(s.height/cell)));
    StemHit hit;hit.distance=1e20;hit.point=float3(0);hit.normal=float3(0,1,0);
    hit.branch=0u;hit.parameter=0;
    bool inside=false;float connectedExit=-1.0;
    if(abs(rd.y)<1e-8) {
        // A horizontal ray can lie exactly on a shared Y face. Merge both rows
        // together, rather than arbitrarily returning the first row's hit.
        StemBlockInterval horizontal[6];uint count=0u;
        for(uint row=0u;row<rows;row++)for(uint branch=0u;branch<(s.branches!=0u?3u:1u);branch++) {
            StemBlockInterval interval=stemBlockInterval(s,ro,rd,row,branch,cell);
            if(interval.hi>=interval.lo&&interval.hi>1e-5&&count<6u)horizontal[count++]=interval;
        }
        for(uint i=0u;i<count;i++)for(uint j=i+1u;j<count;j++)if(horizontal[j].lo<horizontal[i].lo) {
            StemBlockInterval temporary=horizontal[i];horizontal[i]=horizontal[j];horizontal[j]=temporary;
        }
        for(uint i=0u;i<count;i++)if(stemBlockConsume(horizontal[i],ro,rd,inside,connectedExit,hit))return hit;
        if(inside)hit.point=ro+rd*hit.distance;return hit;
    }
    // Wind preserves Y ordering. Scan slabs in ray order, then sort the three
    // branch intervals within each slab. This closes internal voxel/branch
    // faces even for a camera inside the solid, without unbounded ray marching.
    for(uint index=0u;index<rows;index++) {
        uint row=rd.y>=0.0?index:rows-1u-index;
        StemBlockInterval intervals[3];
        for(uint branch=0u;branch<3u;branch++) {
            intervals[branch]=stemBlockInterval(s,ro,rd,row,branch,cell);
            if(branch>0u&&s.branches==0u){intervals[branch].lo=1e20;intervals[branch].hi=-1e20;}
        }
        for(uint i=0u;i<2u;i++)for(uint j=i+1u;j<3u;j++) {
            if(intervals[j].lo<intervals[i].lo) {
                StemBlockInterval temporary=intervals[i];intervals[i]=intervals[j];intervals[j]=temporary;
            }
        }
        for(uint i=0u;i<3u;i++) {
            StemBlockInterval interval=intervals[i];
            if(stemBlockConsume(interval,ro,rd,inside,connectedExit,hit))return hit;
        }
    }
    if(inside)hit.point=ro+rd*hit.distance;
    return hit;
}
