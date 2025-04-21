import numpy as np
import matplotlib.pyplot as plt
import time

# --- SDF Primitive: tapered segment with end‐caps --------------------------

def sdf_tapered_segment(p, a, b, r0, r1, mode='linear'):
    """
    Signed‐distance to a “tapered” capsule from a->b.
    - p: (...,3) sample points
    - a,b: 3‐vectors (segment endpoints)
    - r0: radius at a
    - r1: radius at b
    - mode: 'linear' or 'quadratic' interpolation of radius along t∈[0,1]
    """
    ab = b - a
    ab2 = ab.dot(ab)
    ap = p - a

    # projection t on infinite line
    t_raw = np.sum(ap * ab, axis=-1) / ab2
    # masks for end‐caps vs body
    mask0 = t_raw <= 0
    mask1 = t_raw >= 1
    maskb = (~mask0) & (~mask1)

    # clamp t to [0,1] for body 
    t = np.clip(t_raw, 0.0, 1.0)

    # compute point on axis
    proj = a + t[...,None] * ab
    # radial distance to axis
    d_rad = np.linalg.norm(p - proj, axis=-1)

    # interpolated radius R(t)
    if mode == 'linear':
        R = r0 + (r1 - r0) * t
    elif mode == 'quadratic':
        R = r0 + (r1 - r0) * (t**2)
    else:
        raise ValueError("mode must be 'linear' or 'quadratic'")

    # SDF on body portion
    d_body = d_rad - R

    # SDF to end‐cap spheres
    d_a = np.linalg.norm(p - a, axis=-1) - r0
    d_b = np.linalg.norm(p - b, axis=-1) - r1

    # compose final: use end‐cap SDF where appropriate
    d = np.empty_like(d_body)
    d[mask0] = d_a[mask0]
    d[mask1] = d_b[mask1]
    d[maskb] = d_body[maskb]
    return d

# --- Trunk Generator ------------------------------------------------------

def generate_trunk_voxels(
    height, r_bot, r_tip, interp_mode,
    split_count, split_ratio=0.8,
    split_separate=30, split_bend=30,
    split_angle=0, split_r_bot=None,
    chunk_size=(128,128,128), chunk_origin=(-64,0,-64)
):
    if split_r_bot is None:
        split_r_bot = r_bot * 0.5

    # build grid
    nx,ny,nz = chunk_size
    ox,oy,oz = chunk_origin
    xs = np.arange(nx)+0.5+ox
    ys = np.arange(ny)+0.5+oy
    zs = np.arange(nz)+0.5+oz
    X,Y,Z = np.meshgrid(xs, ys, zs, indexing='ij')
    P = np.stack([X,Y,Z], axis=-1)

    t0 = time.perf_counter()

    # main trunk: from origin to either tip or split‐point
    if split_count == 0:
        A = np.array([0.0, 0.0, 0.0])
        B = np.array([0.0, height, 0.0])
        d = sdf_tapered_segment(P, A, B, r_bot, r_tip, mode=interp_mode)
    else:
        # trunk up to split
        Hs = split_ratio * height
        A = np.array([0.0, 0.0, 0.0])
        B = np.array([0.0, Hs, 0.0])
        d_main = sdf_tapered_segment(P, A, B, r_bot, split_r_bot, mode=interp_mode)

        # two sibling splits
        P0 = B.copy()
        Lb = height - Hs
        θ  = np.deg2rad(split_bend)
        φ0 = np.deg2rad(split_angle)
        δ  = np.deg2rad(split_separate)/2

        d_br = np.full_like(d_main, np.inf)
        for sign in (+1, -1):
            φ = φ0 + sign*δ
            dir_vec = np.array([
                np.sin(θ)*np.cos(φ),
                np.cos(θ),
                np.sin(θ)*np.sin(φ)
            ])
            P1 = P0 + dir_vec * Lb
            d_seg = sdf_tapered_segment(P, P0, P1, split_r_bot, r_tip, mode=interp_mode)
            d_br = np.minimum(d_br, d_seg)

        d = np.minimum(d_main, d_br)

    print(f"[TIMING] SDF computed in {time.perf_counter()-t0:.3f}s")

    vox = np.zeros(chunk_size, dtype=np.uint8)
    vox[d <= 0] = 1
    return vox

# --- Visualization ---------------------------------------------------------

def set_axes_equal(ax):
    x_limits = ax.get_xlim3d()
    y_limits = ax.get_ylim3d()
    z_limits = ax.get_zlim3d()
    x_range = abs(x_limits[1] - x_limits[0])
    y_range = abs(y_limits[1] - y_limits[0])
    z_range = abs(z_limits[1] - z_limits[0])
    max_range = max(x_range, y_range, z_range)
    x_mid = np.mean(x_limits)
    y_mid = np.mean(y_limits)
    z_mid = np.mean(z_limits)
    ax.set_xlim3d(x_mid - max_range/2, x_mid + max_range/2)
    ax.set_ylim3d(y_mid - max_range/2, y_mid + max_range/2)
    ax.set_zlim3d(z_mid - max_range/2, z_mid + max_range/2)

def visualize(vox, title=""):
    pts = np.argwhere(vox==1)
    if pts.size==0:
        print("No voxels to display.")
        return
    xs, ys, zs = pts[:,0], pts[:,1], pts[:,2]
    fig = plt.figure(figsize=(6,6))
    ax  = fig.add_subplot(111, projection='3d')
    ax.scatter(xs, zs, ys, c='sienna', marker='s', s=4, alpha=0.8)
    ax.set_xlabel('X'); ax.set_ylabel('Z'); ax.set_zlabel('Y')
    ax.set_title(title)
    set_axes_equal(ax)
    plt.tight_layout()
    plt.show()

# --- Main: demos -----------------------------------------------------------

if __name__ == "__main__":
    chunk_size   = (128,128,128)
    chunk_origin = (-64, 0, -64)

    examples = [
      ("Linear, no split",
       dict(height=80, r_bot=6, r_tip=2,
            interp_mode='linear', split_count=0,
            chunk_size=chunk_size, chunk_origin=chunk_origin)),
      ("Quadratic, no split",
       dict(height=80, r_bot=6, r_tip=2,
            interp_mode='quadratic', split_count=0,
            chunk_size=chunk_size, chunk_origin=chunk_origin)),
      ("Split@0.8, sep=30°, bend=30°, φ=0°",
       dict(height=80, r_bot=6, r_tip=2,
            interp_mode='linear', split_count=1,
            split_ratio=0.8, split_separate=30,
            split_bend=30, split_angle=0,
            split_r_bot=5,
            chunk_size=chunk_size, chunk_origin=chunk_origin)),
      ("Split@0.5, sep=60°, bend=45°, φ=45°",
       dict(height=80, r_bot=6, r_tip=2,
            interp_mode='linear', split_count=1,
            split_ratio=0.5, split_separate=60,
            split_bend=45, split_angle=45,
            split_r_bot=5,
            chunk_size=chunk_size, chunk_origin=chunk_origin)),
    ]

    for title, params in examples:
        vox = generate_trunk_voxels(**params)
        visualize(vox, title)