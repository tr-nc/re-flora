import matplotlib.pyplot as plt
from matplotlib.widgets import Slider
import numpy as np

# ── GLOBAL PARAMS ────────────────────────────────────────────────────────────
params = {
    'seed':        0,     # randomness seed
    'height':      10.0,  # trunk height
    'split_count': 3,     # 0–10
    'split_ratio': 0.8,   # where along trunk to split (0–1)
    'split_angle': 30.0,  # degrees
    'split_bend':  30.0   # degrees total droop
}

fig = plt.figure(figsize=(10,6))
ax  = fig.add_subplot(111, projection='3d')
fig.subplots_adjust(left=0.05, right=0.65, bottom=0.05, top=0.95)

def draw_tree():
    ax.cla()
    H   = params['height']
    sc  = int(params['split_count'])
    sr  = params['split_ratio']
    θ0  = np.deg2rad(params['split_angle'])
    δ0  = np.deg2rad(params['split_bend'])
    # branch length is the remaining portion of the trunk height
    base_len = H * (1.0 - sr)

    # RNG with fixed seed
    rng = np.random.default_rng(int(params['seed']))
    len_factors    = rng.uniform(0.7, 1.3, sc)
    angle_offsets = np.deg2rad(rng.uniform(-10, 10, sc))
    bend_offsets  = np.deg2rad(rng.uniform(-20, 20, sc))
    phi_offsets   = rng.uniform(-np.pi/12, np.pi/12, sc)

    # 1) Trunk
    Ltrunk = H * sr if sc > 0 else H
    A = np.array([0.0, 0.0, 0.0])
    B = np.array([0.0, Ltrunk, 0.0])

    ax.plot([A[0],B[0]], [A[2],B[2]], [A[1],B[1]],
            color='saddlebrown', lw=2)
    ax.scatter([A[0]],[A[2]],[A[1]], color='saddlebrown', s=30)
    ax.scatter([B[0]],[B[2]],[B[1]], color='saddlebrown', s=30)

    # 2) Branches with t²‑bias in droop
    if sc > 0:
        for i in range(sc):
            φ_base   = 2*np.pi * i / sc
            φ        = φ_base + phi_offsets[i]
            θ_start  = θ0 + angle_offsets[i]
            δ        = δ0 + bend_offsets[i]
            slf      = base_len * len_factors[i]

            n  = 40
            ds = slf / n
            pts = [B.copy()]
            cur = B.copy()

            for k in range(1, n+1):
                t = k / n
                # stronger bend near tip via t²
                θ = θ_start - δ * (t**2)
                v = np.array([
                    np.sin(θ)*np.cos(φ),   # X
                    np.cos(θ),             # Y (vertical)
                    np.sin(θ)*np.sin(φ)    # Z
                ])
                cur = cur + v * ds
                pts.append(cur.copy())

            pts = np.array(pts)
            ax.plot(pts[:,0], pts[:,2], pts[:,1],
                    color='forestgreen', lw=2)
            tip = pts[-1]
            ax.scatter([tip[0]],[tip[2]],[tip[1]],
                       color='forestgreen', s=30)

    # 3) Axes & camera
    lim = H * 1.2
    ax.set_xlim(-lim, lim)
    ax.set_ylim(-lim, lim)
    ax.set_zlim(0, lim)
    ax.set_box_aspect((1,1,1))
    ax.set_xlabel('X')
    ax.set_ylabel('Y (depth)')
    ax.set_zlabel('Z (up)')
    ax.view_init(elev=30, azim=-50)

# initial draw
draw_tree()

# ── SLIDERS ──────────────────────────────────────────────────────────────────
slider_defs = [
    ('seed',        0,   1000, params['seed'],        1),
    ('height',      1.0, 20.0,  params['height'],      0.5 ),
    ('split_count', 0,   10,    params['split_count'], 1   ),
    ('split_ratio', 0.0, 1.0,   params['split_ratio'], 0.01),
    ('split_angle',  0.0,90.0,  params['split_angle'], 1.0 ),
    ('split_bend',  -90.0,90.0, params['split_bend'],   1.0 )
]

sliders = {}
h = 0.03; sp = 0.02; x0, w = 0.70, 0.25
for i, (name, vmin, vmax, vinit, step) in enumerate(slider_defs):
    y0 = 0.95 - (i+1)*(h+sp)
    ax_s = fig.add_axes([x0, y0, w, h], facecolor='lightgray')
    s = Slider(ax_s, name, valmin=vmin, valmax=vmax,
               valinit=vinit, valstep=step)
    sliders[name] = s

def update(_):
    for name, s in sliders.items():
        params[name] = s.val
    draw_tree()
    fig.canvas.draw_idle()

for s in sliders.values():
    s.on_changed(update)

plt.show()