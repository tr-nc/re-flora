# MLS-MPM with CPIC — polished implementation reference

Source paper: Yuanming Hu et al., **A Moving Least Squares Material Point Method with Displacement Discontinuity and Two-Way Rigid Body Coupling**, ACM TOG 37(4), Article 146, 2018.

This reference combines the local raw extraction (`mls-mpm-paper.md`) with the Docling extraction (`mls-mpm-docling.md`). Use it as an implementation guide; keep the PDF as the authority for details.

## Why this paper matters for us

MLS-MPM is an APIC-compatible MPM formulation that:

- reuses the APIC affine matrix as the velocity-gradient estimate;
- avoids explicit B-spline gradient evaluation for elastic forces;
- fuses affine momentum and force terms in P2G;
- is simpler and often faster than traditional MPM;
- can be extended with CPIC for thin cuts, displacement discontinuities, and two-way rigid coupling.

The first useful target for this repo should be **plain explicit MLS-MPM/APIC**. CPIC can come later.

![Fig. 1](mls-mpm-docling_artifacts/image_000000_7fbe51ff097bda7122c70449bdaff0b2c5aa9e1f5bc37d3f1ad4caa36b33674f.png)

## Notation

| Symbol                 | Meaning                                                      |
| ---------------------- | ------------------------------------------------------------ |
| $p$                    | particle index                                               |
| $i,j$                  | grid node indices                                            |
| $x_p, v_p, m_p$        | particle position, velocity, mass                            |
| $x_i, v_i, m_i$        | grid node position, velocity, mass                           |
| $F_p$                  | particle deformation gradient                                |
| $C_p$                  | APIC affine velocity matrix                                  |
| $N_i(x_p)$ or $w_{ip}$ | B-spline weight from particle $p$ to node $i$                |
| $\sigma_p$             | Cauchy stress                                                |
| $V_p^0, V_p^n$         | initial/current particle volume                              |
| $M_p$                  | MLS moment constant, scalar times identity for regular grids |
| $\Delta x, \Delta t$   | grid spacing and timestep                                    |

For regular grids with standard B-splines:

$$
M_p = \frac{1}{4}\Delta x^2 I \quad \text{for quadratic B-splines},
\qquad
M_p = \frac{1}{3}\Delta x^2 I \quad \text{for cubic B-splines}.
$$

Most real-time/simple implementations use quadratic B-splines.

## Core MLS-MPM equations

### MLS reconstruction

For samples $u_i=u(x_i)$, moving least squares reconstructs near $x$ with

$$
u(z)=P^T(z-x)c(x),
$$

where

$$
c(x)=M^{-1}(x)b(x),
$$

$$
b(x)=\sum_{i\in B_x}\xi_i(x)P(x_i-x)u_i,
\qquad
M(x)=\sum_{i\in B_x}\xi_i(x)P(x_i-x)P^T(x_i-x).
$$

The MLS shape function is

$$
\Phi_i(z)=\xi_i(x)P^T(z-x)M^{-1}(x)P(x_i-x).
$$

MLS-MPM uses B-splines as the weighting functions $\xi_i$ and particle locations as local reconstruction centers.

### Force term

The paper derives the MLS-MPM grid force as

$$
f_i = -\sum_p V_p^n M_p^{-1}\sigma_p^n N_i(x_p^n)(x_i-x_p^n).
$$

For a hyperelastic energy $E=\sum_p V_p^0\Psi_p(F_p)$, this is equivalently

$$
f_i=-\sum_p N_i(x_p^n)V_p^0M_p^{-1}
\frac{\partial\Psi}{\partial F}(F_p^n)(F_p^n)^T(x_i-x_p^n).
$$

This replaces the traditional MPM expression involving $\nabla N_i(x_p)$.

### Deformation gradient update

APIC already computes the affine velocity matrix $C_p$. MLS-MPM reuses it:

$$
\frac{\partial \hat v^{n+1}}{\partial x}=C_p^{n+1},
\qquad
F_p^{n+1}=\left(I+\Delta t\,C_p^{n+1}\right)F_p^n.
$$

No separate grid-kernel-gradient gather is needed.

## Explicit APIC/MLS-MPM algorithm

### 1. Clear grid

For every active grid node:

```text
m_i = 0
(mv)_i = 0
f_i = 0       # optional if using unfused force scatter
```

### 2. Particle to grid, fused MLS-MPM form

Define

$$
Q_p = \Delta t\,V_p^0M_p^{-1}\frac{\partial\Psi}{\partial F}(F_p^n)(F_p^n)^T + m_p C_p^n.
$$

For each particle-node pair in the B-spline support:

$$
m_i \mathrel{+}= w_{ip}m_p,
$$

$$
(mv)_i \mathrel{+}= w_{ip}\left(m_p v_p^n + Q_p(x_i-x_p^n)\right).
$$

This combines APIC affine momentum and the stress force. The sign convention above matches the paper's fused form because the force is applied as $+\Delta t f_i$ and $f_i$ already contains the negative energy gradient.

### 3. Grid update

For nodes with mass:

$$
v_i = \frac{(mv)_i}{m_i},
$$

then apply body forces and boundary conditions:

$$
v_i \mathrel{+}= \Delta t\,g.
$$

For non-fused implementations, use:

$$
(m\hat v)_i^{n+1}=(mv)_i^n+\Delta t(m_i^n g+f_i^n).
$$

### 4. Grid to particle, APIC gather

Gather updated particle velocity and affine matrix:

$$
v_p^{n+1}=\sum_i w_{ip}v_i^{n+1},
$$

$$
C_p^{n+1}=D_p^{-1}\sum_i w_{ip}v_i^{n+1}(x_i-x_p^n)^T.
$$

For quadratic/cubic B-splines on regular grids, $D_p$ is the same constant moment matrix as $M_p$ up to the convention used in the APIC implementation.

### 5. Update particle state

$$
F_p^{n+1}=\left(I+\Delta t C_p^{n+1}\right)F_p^n,
$$

then apply plasticity projection if needed, then advect:

$$
x_p^{n+1}=x_p^n+\Delta t\,v_p^{n+1}.
$$

## Constitutive model reminder

For fixed-corotated elasticity, commonly used in graphics MPM:

$$
\Psi(F)=\mu\lVert F-R\rVert_F^2+\frac{\lambda}{2}(J-1)^2,
\qquad J=\det F,
$$

where $R$ is the rotational part of $F$ from polar decomposition. The first Piola stress is typically written:

$$
P(F)=\frac{\partial\Psi}{\partial F}=2\mu(F-R)+\lambda(J-1)J F^{-T}.
$$

Then the fused MLS-MPM matrix uses $P(F_p^n)(F_p^n)^T$.

## CPIC extension summary

CPIC adds compatibility between particles and grid nodes so material on opposite sides of a thin boundary does not communicate through the same grid velocity field.

![Fig. 10](mls-mpm-docling_artifacts/image_000009_6b62a283d55cd0e1a8a9528497313cd28de5c94b2cb233dfc48e715753d62b1e.png)

### Colored distance field

For rigid surface samples $r_\eta$, splat signed point-plane distances $u_{i,r_\eta}$ to nearby grid nodes. The unsigned grid distance is

$$
d_i = \min_{r,\eta}|u_{i,r_\eta}|.
$$

The grid stores, per nearby rigid surface $r$:

$$
A_{ir}=\begin{cases}
1,&\exists\eta\text{ with valid }u_{i,r_\eta},\\
0,&\text{otherwise,}
\end{cases}
$$

and a side tag

$$
T_{ir}=\operatorname{sign}(u_{i,r_{\eta^*}}(x_i)).
$$

Here $A$ is affinity and $T$ labels which side of the boundary the node is on.

![Fig. 12](mls-mpm-docling_artifacts/image_000011_c528941699cf598d492788cf5c6855c8bcb17dd3d575edb113c26b0e03845b5a.png)

### Particle color and normal

A particle inherits affinities from nodes in its support. Its tag is a distance-weighted vote:

$$
T_{pr}=\operatorname{sign}\left(\sum_i N_i(x_p)d_iT_{ir}\right).
$$

For distance and normal, locally convert unsigned grid distances into signed distances using the particle/grid tags, then apply the MLS reconstruction from the paper. The particle normal is

$$
n_p=\frac{\nabla d_p}{\lVert\nabla d_p\rVert}.
$$

If a particle penetrates, keep its previous color/tag and apply a weak penalty force:

$$
f_p^{P,n}=-k_h d_p n_p.
$$

### Compatibility rule

Let $S_i$ be surfaces with nonzero affinity at grid node $i$, and $S_p$ be surfaces with nonzero affinity at particle $p$.

A particle $p$ and grid node $i$ are compatible iff

$$
T_{ir}=T_{pr}\quad\forall r\in S_i\cap S_p.
$$

Use $i_p^+$ for compatible nodes and $i_p^-$ for incompatible nodes.

### CPIC particle to grid

Only compatible particles transfer mass and momentum to a node:

$$
m_i^n=\sum_{q\in p_i^+}N_i(x_q^n)m_q,
$$

$$
(mv)_i^n=\sum_{q\in p_i^+}N_i(x_q^n)m_q\left(v_q^n+C_q^n(x_i-x_q^n)\right).
$$

For each incompatible node contribution, project the particle velocity against the closest rigid boundary and apply the missing impulse to the rigid body.

### Boundary velocity projection

Rigid body world velocity at point $x$:

$$
V_r^n(x)=v_r^n+\omega_r^n\times(x-x_r^n).
$$

Let $v$ be relative velocity against the boundary and $n$ the boundary normal. Define

$$
v_t=v-(v\cdot n)n,
\qquad
\hat v_t=\frac{v_t}{\lVert v_t\rVert},
\qquad
\zeta=\max(0,\lVert v_t\rVert+\mu v\cdot n).
$$

Projection:

$$
\operatorname{Proj}(v,n,B,\mu)=
\begin{cases}
0,&B=\text{sticky},\\
v_t,&B=\text{slip},\\
\zeta\hat v_t,&B=\text{separate and }v\cdot n\le 0,\\
v,&B=\text{separate and }v\cdot n>0.
\end{cases}
$$

Boundary-relative particle projection:

$$
\operatorname{Proj}_r(v_p^n,n_p,x_i)=V_r^n(x_i)+
\operatorname{Proj}(v_p^n-V_r^n(x_i),n_p,B_r,\mu_r).
$$

### CPIC grid to particle

For incompatible nodes, use a ghost velocity $\tilde v_p$ rather than the node velocity:

$$
v_p^{n+1}=\sum_{j\in i_p^-}N_j(x_p^n)\tilde v_p+
\sum_{j\in i_p^+}N_j(x_p^n)\hat v_j^{n+1}.
$$

The APIC affine gather becomes

$$
C_p^{n+1}=D_p^{-1}\left(
\sum_{j\in i_p^-}N_j(x_p^n)\tilde v_p(z_{jp}^n)^T+
\sum_{j\in i_p^+}N_j(x_p^n)\hat v_j^{n+1}(z_{jp}^n)^T
\right),
$$

where

$$
z_{jp}^n=x_j-x_p^n.
$$

Then use the same MLS-MPM deformation-gradient update and particle advection as usual.

## Performance notes from the paper

The main win is algorithmic: MLS-MPM avoids kernel gradients and fuses force with affine momentum. The paper reports roughly a 2× direct algorithmic speedup for explicit integration, before additional low-level optimization.

Table 2 from the paper reports the following transfer timings for 8 million particles on a 4-core i7-7700K:

| Timing         | Reference | Ours MPM | Ours optimized MPM | Ours optimized MLS-MPM |
| -------------- | --------: | -------: | -----------------: | ---------------------: |
| P2G, 1 thread  |   4760 ms |  5744 ms |            2685 ms |                1283 ms |
| P2G, 4 threads |   1220 ms |  1525 ms |             688 ms |                 328 ms |
| G2P, 1 thread  |   8255 ms |  7476 ms |            1144 ms |                 589 ms |
| G2P, 4 threads |   2070 ms |  2011 ms |             313 ms |                 163 ms |

CPIC overhead was low in their examples because only a narrow band around rigid boundaries needs compatibility handling.

## Suggested implementation sequence for this repo

1. **Baseline particle/grid data**: position, velocity, mass, $F$, $C$, volume, material params.
2. **Quadratic B-spline weights**: 2D first, then 3D if needed.
3. **MLS-MPM P2G fused scatter**: mass + momentum using $Q_p$.
4. **Grid update**: gravity and simple boundaries.
5. **APIC G2P**: gather velocity and $C_p$.
6. **Deformation update**: $F\leftarrow(I+dt C)F$.
7. **Material model**: fixed-corotated elastic first; add plasticity later.
8. **Perf pass**: active-node lists, tiled/blocked grid, avoid atomics where possible.
9. **Optional CPIC**: CDF, particle/grid compatibility, ghost velocities, rigid impulses.

## Caveats

- CPIC resolves thin boundaries at grid scale, not arbitrary sub-grid topology.
- The compatibility condition is binary and effectively grid-aligned.
- Sharp reconstructed cut surfaces still need extra surface extraction work.
- Strong fully implicit rigid-MPM coupling is not covered by the paper.
- For GPU implementation, measure memory layout and stride choices rather than assuming smaller is faster.
