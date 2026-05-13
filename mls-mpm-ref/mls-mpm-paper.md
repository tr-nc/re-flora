# A Moving Least Squares Material Point Method with Displacement Discontinuity and Two-Way Rigid Body Coupling

> Markdown text extraction from `mls-mpm.pdf` for local implementation reference.

Source PDF metadata: Yuanming Hu, Yu Fang, Ziheng Ge, Ziyin Qu, Yixin Zhu, Andre Pradhana, and Chenfanfu Jiang. ACM TOG 37(4), Article 146, 2018.

```text

A Moving Least Squares Material Point Method with Displacement
Discontinuity and Two-Way Rigid Body Coupling
YUANMING HU† , MIT CSAIL
YU FANG† , Tsinghua University
ZIHENG GE† , University of Science and Technology of China
ZIYIN QU, University of Pennsylvania
YIXIN ZHU† , University of California, Los Angeles
ANDRE PRADHANA, University of Pennsylvania
CHENFANFU JIANG, University of Pennsylvania




         Fig. 1. Our method allows MPM to handle world space material cutting, complex thin boundaries and natural two-way rigid body coupling.

In this paper, we introduce the Moving Least Squares Material Point Method                        Method with Displacement Discontinuity and Two-Way Rigid Body Cou-
(MLS-MPM). MLS-MPM naturally leads to the formulation of Affine Particle-                         pling. ACM Trans. Graph. 37, 4, Article 146 (August 2018), 14 pages.
In-Cell (APIC) [Jiang et al. 2015] and Polynomial Particle-In-Cell [Fu et al.                     DOI: 10.1145/3197517.3201293
2017] in a way that is consistent with a Galerkin-style weak form discretiza-
tion of the governing equations. Additionally, it enables a new stress di-
vergence discretization that effortlessly allows all MPM simulations to run                       1    INTRODUCTION
two times faster than before. We also develop a Compatible Particle-In-Cell
(CPIC) algorithm on top of MLS-MPM. Utilizing a colored distance field rep-                       Since the pioneering work of Terzopoulos et al. [1988], simulat-
resentation and a novel compatibility condition for particles and grid nodes,                     ing topologically changing materials has been a popular research
our framework enables the simulation of various new phenomena that are                            topic in graphics. Among various topics, fracture and cutting of
not previously supported by MPM, including material cutting, dynamic open                         deformable objects are most intensively explored. The goal of break-
boundaries, and two-way coupling with rigid bodies. MLS-MPM with CPIC                             ing mesh connectivity has led to techniques such as local remesh-
is easy to implement and friendly to performance optimization.                                    ing [O’Brien et al. 2002; O’Brien and Hodgins 1999], the Virtual Node
CCS Concepts: • Computing methodologies → Physical simulation;                                    Algorithm (VNA) [Hegemann et al. 2013; Molino et al. 2005; Wang
                                                                                                  et al. 2014] and the eXtended Finite Element Method (XFEM) [Koschier
Additional Key Words and Phrases: Material Point Method (MPM), moving
                                                                                                  et al. 2017]. Maintaining remeshing quality efficiently and robustly
least squares, cutting, discontinuity, distance field, rigid coupling
                                                                                                  can be however very complicated. While VNA and XFEM reduce
ACM Reference format:                                                                             some difficulty, they impose additional challenges like floating point
Yuanming Hu† , Yu Fang† , Ziheng Ge† , Ziyin Qu, Yixin Zhu† , Andre Prad-                         arithmetic in degenerate scenarios and self-collision on embedded
hana, and Chenfanfu Jiang. 2018. A Moving Least Squares Material Point                            surfaces.
Permission to make digital or hard copies of all or part of this work for personal or                Compared to mesh-based approaches, meshless animation of solid
classroom use is granted without fee provided that copies are not made or distributed             topology change was shown to be promising by Pauly et al. [2005].
for profit or commercial advantage and that copies bear this notice and the full citation
on the first page. Copyrights for components of this work owned by others than ACM
                                                                                                  More recently, the Material Point Method (MPM) [Sulsky et al. 1995]
must be honored. Abstracting with credit is permitted. To copy otherwise, or republish,           emerged as an effective choice for various materials and gained
to post on servers or to redistribute to lists, requires prior specific permission and/or a
fee. Request permissions from permissions@acm.org.
© 2018 ACM. 0730-0301/2018/8-ART146 $15.00                                                        † Y. Hu, Y. Fang, Z. Ge and Y. Zhu were visiting students at the University of Pennsyl-
DOI: 10.1145/3197517.3201293                                                                      vania during this work.


                                                                                              ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
146:2 •      Y. Hu et al.


popularity in VFX and in animations such as Disney’s Frozen [Stom-                          fracture [2002] via locally remeshing tetrahedral elements according
akhin et al. 2013]. Utilizing both meshless Lagrangian particles                            to the embedded fracture surface. Their algorithm preserves the
and a background Eulerian grid, MPM has become advantageous                                 orientation of fracture surfaces during the remeshing process. Bao
in simulating multi-physics phenomena, as shown in [Stomakhin                               et al. [2007] presented a novel algorithm for efficient fracture of
et al. 2014] and [Tampubolon et al. 2017]. In contrast to FEM, MPM                          nearly rigid materials. Kaufmann et al. [2009] used Discontinuous
automatically supports arbitrarily extreme topologically changing                           Galerkin Finite Element Method (DGFEM) for handling disconti-
dynamics including material split and merge. It also does not suffer                        nuities. Hegemann et al. [2013] minimizes the Griffith’s energy
from boundary difficulties such as the tensile instability in Smoothed                      for ductile fracture of embedded level sets. Pauly et al. [2005] pre-
Particle Hydrodynamics (SPH).                                                               sented a meshless framework for elastoplastic fracture, where ex-
   Despite its high efficacy in many situations, traditional MPM fails                      plicit crack surfaces are initiated with stress criteria on particles.
to model sharp separation of material points and cannot represent                           Chen et al. [2014] developed an efficient adaptive remeshing method
discontinuous velocities. We show a 2D cutting example in Fig. 2.                           based on gradient descent flow, which automatically refines fracture
Directly colliding with a thin level set (Fig. 2a) fails due to the                         surfaces. Pfaff et al. [2014] also performed adaptive remeshing for
under-resolution of the collision object. Plastic softening (Fig. 2b),                      fracturing thin sheets. Hahn and Wojtan [2015; 2016] used Bound-
which is common for modeling material failure, results in too many                          ary Element Method (BEM) and Lagrangian crackfronts to produce
debris particles. Finally, particle deletion (Fig. 2c) requires removing                    highly detailed fracture surfaces.
a noticeable number of particles, which causes visual artifacts. The                           For material cutting, the Virtual Node Algorithm (VNA) by Molino
main issue is that particle kernels in MPM are nonconforming to                             et al. [2005] duplicates (instead of splitting) simulation elements
boundaries. Each particle exchanges data with grid nodes in its entire                      that intersect the cutting geometry. The original VNA only allows
kernel support across the boundary which naturally causes velocity                          one cut per face and does not handle degenerate cases. To over-
field smoothing. This issue is more pronounced in MPM than FLIP-                            come these shortcomings, Sifakis et al. [2007] improved it to allow
based fluids [Zhu and Bridson 2005] due to the wider kernel support                         arbitrarily generalized cutting surfaces at smaller scales than tetra-
of quadratic or cubic B-splines required for solids [Steffen et al.                         hedron resolution. Wang et al. [2014] further developed a robust
2008].                                                                                      adaptive VNA with robust floating point arithmetic for degener-
                                                                                            ate intersections. Recently, Koschier et al. [2017] presented a new
1.1    Contributions                                                                        remeshing-free cutting algorithm with XFEM, which was shown to
For resolving these fundamental issues, we develop a Compatible                             better preserve physical plausibility such as mass conservation and
Particle-In-Cell (CPIC) algorithm that allows for material point dis-                       correctly maintained stiffness properties. We refer to the survey by
continuity and infinitely thin boundaries based on relative locations                       Wu et al.[2015] for more detailed previous work on cutting.
between particles and grid nodes. Unlike node-visibility-based al-
gorithms that are common in element-free Galerkin (EFG) crack
                                                                                            2.2   Fluid Boundaries and Rigid-Fluid Coupling
simulations [Belytschko et al. 1994; Belytschko and Tabbara 1996]                           While there exists a lot of previous work on resolving solid fluid
or the transparency method used by Pauly et al. [2005], our formu-                          interaction and complex boundaries, we will focus on reviewing
lation does not require any expensive ray mesh intersection queries.                        the treatment of thin shell rigid boundaries, which is most rele-
Additionally, CPIC facilitates two-way rigid-MPM coupling in a                              vant to our work. Carlson et al. [2004] presented the Rigid Fluid
straightforward fashion.                                                                    method where rigid bodies are resolved on the Eulerian grid through
   Our framework is based on a novel weak form discretization of                            a rigidity projection. This approach works best when the rigid body
MPM. We show that the low dissipation Affine PIC[Jiang et al. 2015,                         is not extremely thin. The first work considering thin solid/fluid
2017b] and Polynomial PIC [Fu et al. 2017] methods can be derived                           coupling is by Guendelman et al. [2005]. They used a robust ray
from a Galerkin-style Moving Least Squares (MLS) discretization of                          casting algorithm to augment the velocity interpolation and ker-
the governing equations. We extend the idea and use MLS to further                          nel computation near surfaces. Later work further improved the
replace the shape functions in the stress divergence term. This leads                       stability [Robinson-Mosher et al. 2008; Shinar et al. 2008] and accu-
to a new force computation scheme that does not require evaluating                          racy [Robinson-Mosher et al. 2009] near boundaries. Chentanez et
the gradients of nodal shape functions. Compared to traditional                             al. [2006] combined fluid pressure projection and elasticity integra-
MPM, the resulting Moving Least Squares Material Point Method                               tion into simultaneous equations and enabled the usage of large time
(MLS-MPM) provides almost identical visual results and enables an                           steps. To obtain higher accuracy of boundary handling, Klingner et
effortless 2× speed up with easier implementation.                                          al. [2006] proposed two-way rigid-fluid coupling based on conform-
                                                                                            ing unstructured meshes and remeshing. Feldman et al. [2005] also
2 RELATED WORK                                                                              adopted boundary conforming tetrahedral meshes for discretizing
                                                                                            the domain. However these approaches have not been investigated
2.1 Deformable Objects Fracture and Cutting                                                 for treating thin shell, dynamic, rigid bodies. Batty et al. [2007]
Fracture simulation was pioneered by Terzopoulos et al. [1988]. With                        proposed a variational pressure projection (at sub-grid resolution)
FEM, the most simple and efficient approach for handling cutting                            to account for partial cell volume. Narain et al. [2010] adopted this
and fracture is to split surfaces along element boundaries [Müller                          formulation for coupling frictional stress of granular media with
and Gross 2004]. A more accurate strategy splits individual ele-                            rigid bodies. Azevedo et al. [2016] extended the cut-cell approach
ments, as pioneered by O’Brien et al. for brittle [1999] and ductile                        to enable one way coupling between hybrid Lagrangian/Eulerian

ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
           A Moving Least Squares Material Point Method with Displacement Discontinuity and Two-Way Rigid Body Coupling • 146:3




Fig. 2. (a) Traditional level set collision objects cannot cut the elastic object even if they only cover one layer of grid nodes; (b) Plastic softening allows material
separation, but introduces a lot of visually unappealing damaged debris; (c) Particle deletion either does not work or causes too much volume loss; (d) Our
method successfully handles both progressive and instant cutting.


fluids with arbitrarily thin solid boundary obstacles. They also ad-
dressed the treatment of thin gaps between multiple objects. Zarifi
et al. [2017] developed positive-definite cut-cell method for strong
coupling between elastic objects and incompressible fluids.
   Particle-based methods are also popular in fluid simulation. Tra-
ditional Smoothed Particle Hydrodynamics (SPH) [Müller et al.
2003] provides very limited control over solid boundaries. Becker
et al. [2009] achieved two-way coupling of compressible SPH and
rigid bodies by sampling boundary particles on rigid bodies and                              Fig. 4. An elastic bunny is split by two intersecting thin plates.
using a predictor-corrector scheme to compute forces on particles.
Akinci et al. [2012] also sampled particles on rigid boundaries, but
proposed a more versatile method that handles pressure and fric-
tion directly with hydrodynamic forces. Their approach works well
for thin shell rigid bodies. Koschier et al. [2017] recently proposed                  2.3     Material Point Method
density maps for SPH boundaries. Using precomputed density maps,
                                                                                       MPM [Sulsky et al. 1995] is a hybrid Lagrangian/ Eulerian discretiza-
their approach eliminated the need for sampling rigid boundary
                                                                                       tion scheme for solid mechanics. It is also recognized as a generaliza-
particles. Macklin et al. [2013] proposed position-based fluids un-
                                                                                       tion of the FLIP [Brackbill and Ruppel 1986] method, which is widely
der the position-based dynamics (PBD) framework [Müller et al.
                                                                                       used for liquid animation [Zhu and Bridson 2005]. More recently,
2007] where collisions against boundaries are formulated as non-
                                                                                       MPM has been applied to various computer graphics applications
penetration constraints. The approach is further extended in [Mack-
                                                                                       including snow [Stomakhin et al. 2013], sand [Daviet and Bertails-
lin et al. 2014] to allow solid-fluid coupling through density con-
                                                                                       Descoubes 2016; Klár et al. 2016], foam [Ram et al. 2015; Yue et al.
straints.
                                                                                       2015], cloth [Jiang et al. 2017a], and solid-fluid mixture [Stomakhin
                                                                                       et al. 2014; Tampubolon et al. 2017]. Notably, Daviet et al. [2016]
                                                                                       presented a semi-implicit frictional boundary condition for coupling
                                                                                       MPM sand with rigid bodies. Due to the adoption of a single veloc-
                                                                                       ity field, it remains challenging to separate continuum materials in
                                                                                       MPM. Wretborn et al. [2017] animated MPM crack propagation by
                                                                                       gluing multiple grids together. It assumes pre-fracturing and purely
                                                                                       elastic materials. In engineering literature, MPM material disconti-
                                                                                       nuity is sometimes achieved by explicit front tracking [Nairn 2003],
                                                                                       which performs multiple ray intersection tests per particle across an
                                                                                       explicit mesh. Most other approaches achieve material separation
                                                                                       through strain softening or material damaging [Banerjee et al. 2012].
                                                                                       This is similar to element deletion in FEM, and causes mesh depen-
                                                                                       dent volume loss as well as undesirable debris. Gao et al. [2017]
                                                                                       developed spatially adaptive MPM. They resolved thin features by
                                                                                       locally refining the computational grid and particles. Moutsanidis
                                                                                       et al. [2018] modeled strong discontinuities in MPM using a sin-
                                                                                       gle velocity field. They locally modify interpolation functions near
Fig. 3. Cutting cheese with a wavy knife shows appealing peeling behavior.             discontinuity.


                                                                                   ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
146:4 •      Y. Hu et al.


2.4    Moving Least Squares                                                                  Variable     Type    Meaning
As a local fitting scheme, Moving Least Squares (MLS) has been                                   u        any     any continuous function approximated with MLS
                                                                                                xi       vector   the location of sample/node i
widely adopted in computer graphics, including for image deforma-                               xp       vector   the location of particle p
tion [Kanamori et al. 2011; Schaefer et al. 2006], surface reconstruc-                         z, x      vector   an arbitrary continuous location
                                                                                              P(x)       vector   the polynomial basis
tion [Lancaster and Salkauskas 1981; Levin 2004], particle-based                               c(x)      vector   all basis coefficients
simulation [Band et al. 2017; Müller et al. 2004], and data compres-                          M(x)       matrix   the moment matrix
                                                                                               Mp        matrix   M(xp )
sion [Langlois et al. 2014]. We refer to Levin’s book [Levin 1998] for                        ξ i (x)    scalar   weighting function centered at xi
a more detailed introduction to MLS.                                                          Φi (x)     scalar   MLS shape function centered at xi
   MLS is most popular in the areas of deformation and surface                                N i (x)    scalar   B-spline basis function centered at xi
                                                                                             ρ (x, t )   vector   the continuous density field
reconstruction. The pioneering work of Schaefer et al. [2006] de-                             v(x)       vector   the continuous velocity field
rived a closed-form expression for least-squares image deformation.                            mi        scalar   mass of node i
                                                                                                vi       vector   velocity of node i
Kanamori et al. [2011] extended this idea to reduce distortion in                               vni      vector   velocity of node i at time n over domain Ω t
                                                                                                                                                               n

                                                                                                                                                                  n
wide-angle images. Zhu et al. [2007] generalized it to 3D deforma-                              v̂ni     vector   velocity of node i at time n + 1 over domain Ωt
tion problems. Sato et al. [2014] proposed a relevant method for                               mp        scalar   mass of particle p
deforming fluid flow fields based on physical laws. As for surface                              vp       vector   velocity of particle p
                                                                                                Cp       matrix   affine matrix of particle p
reconstruction, Levin et al. [2004] introduced a MLS projection                                  q       vector   test function in the weak form
procedure for constructing smooth surfaces from potentially noisy                             qα , β     scalar   derivative of q α wrt. x β
                                                                                                 σ       matrix   Cauchy stress
point cloud data. Fleishman et al. [2005] augmented this algorithm                              Fp       matrix   the deformation gradient on particle p
with robust statistical tools, yielding piecewise smooth surfaces.                               fi      vector   the force on grid node i
   MLS has also been applied to particle-based simulations, espe-                             Table 1. Important notations used in the MLS-MPM derivation (§3).
cially for interpolating continuous functions on sampled particles.
Müller et al. [2004] provided a local MLS approximation to the
gradient of the displacement field for evaluating stress, strain and
other mechanical values. Pauly et al. [2005] extended their work to
modeling fracture surfaces. Martin et al. [2010] used Generalized
Moving Least Squares (GMLS) for discretizing the displacement
field in elastica. More recently, Band et al. [2017] embedded MLS
into SPH boundary handling. Their method allows particles to slip
along boundaries without any distortion. Plus, MLS is also powerful
in data compression. For example Langlois et al. [2014] presented
an eigenmode compression of modal sound based on non-linear
optimization of MLS.
   MLS is a core idea behind meshless methods such as the element-
free Galerkin (EFG) method [Belytschko et al. 1994; Huerta et al.
2004] and the Reproducing Kernel Particle Method (RKPM) [Liu et al.
1995]. In §3 we review the main idea of MLS function reconstruction,
and derive APIC, PolyPIC and MLS-MPM from this point of view.                               Fig. 5. Two dimensional sand inflow is two-way coupled with a wheel. The
                                                                                            wheel is made of intersecting thin boundaries, allowing clean separation
3     MOVING LEAST SQUARES MPM                                                              for materials on different sides and corners.

In this section we derive MLS-MPM as a new spatial discretization
that unifies APIC, PolyPIC and force computation consistently with                             Suppose one is given a set of samples at some locations xi for
the weak form of the momentum equation. Interestingly, our deriva-                          a continuous function ui = u (xi ), the idea behind MLS [Lancaster
tion shows that MPM, although seemingly quite different from
purely Lagrangian meshless methods, can be treated as a modified
element-free Galerkin (EFG) method [Belytschko et al. 1994], where
the background Eulerian grid merely acts as a helper structure for
accelerating MLS interpolation from particle neighbor regions.
   We use subscript i to denote grid node quantities and p to denote
particle quantities. We provide a list of important notations used in
this section in Table 1.

3.1    Discrete MLS in element-free Galerkin
We start with reviewing MLS in purely meshless methods such as
element-free Galerkin (EFG) [Belytschko et al. 1994]; see [Huerta                           Fig. 6. Our method enables two-way coupled simulation of splashing water
et al. 2004] for more details.                                                              and rigid blocks with different densities.


ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
            A Moving Least Squares Material Point Method with Displacement Discontinuity and Two-Way Rigid Body Coupling • 146:5


and Salkauskas 1981] is that for a fixed x, one can approximate u                              3.2    Equivalence of APIC/PolyPIC and MLS on velocities
at any location z in the continuous space near x using a polyno-                               MPM discretizes the governing equations using interpolation func-
mial least-squares fit of u from the samples in this local region                              tions on the grid as shape functions and particles as quadrature
with u (z) = PT (z)c(x), where P(z) = [p0 (z), p1 (z), . . . , pl (z)]T                        points. Each particle (subscripted with p) has mass mp , position xp ,
forms an l dimensional subspace of polynomials of degree m, and                                velocity vp , deformation gradient Fp and other parameters related
c(x) = [c 0 (x), c 1 (x), . . . , cl (x)]T are the basis coefficients. In prac-                to its material constitutive model. A grid (subscripted with i) acts
tice, to avoid numerical instabilities caused by large entries in the mo-                      as a scratch pad and stores mass mi and velocity vi . In each time
ment matrix (see M below), the polynomial basis can be re-centered                             step, particles transfer mass and velocity to the grid. Grid velocity
around a fixed point x by replacing PT (z) with PT (z − x) (see e.g.                           is then integrated over time and transferred back to particles.
[Liu et al. 1995]), leading to                                                                    APIC [Jiang et al. 2015] and PolyPIC [Fu et al. 2017] are equivalent
                               u (z) = PT (z − x)c(x).                              (1)        to applying MLS to velocity v(x) with B-splines as the weighting
                                                                                               function ξ i (x). We give a detailed discussion in the supplementary
In EFG, c(x) is evaluated using weighted least squares that min-                               document [Hu et al. 2018]. Unlike EFG or RKPM where reconstruc-
                                                                     2
imizes the functional Jx (c) = i ∈Bx ξ i (x) PT (xi − x)c(x) − ui ,
                                               
                                                                                               tion and data sample locations are colocated, MPM uses Cartesian
                                  P
where ξ i (x) is a localized weighting function centered at xi , and B x                       lattice nodes for data samples without particle neighbor search.
denotes the set of indices satisfying ξ i (x) , 0. The solution is
                                                                                               3.3    MLS-MPM as a special EFG discretization
                                c(x) = M−1 (x)b(x),                                 (2)
                                                                                               In this section we derive the MLS-MPM discretization from the con-
where b(x) = i ∈Bx ξ i (x)P(xi −x)ui and M(x) = i ∈Bx ξ i (x)P(xi −
              P                                   P
                                                                                               tinuous weak form of the governing equations. Implicit summation
   T
x)P (xi − x). Note that when P only contains linear polynomials, a                             convention on indices is assumed.
more intuitive form of c(x) is given in §3.1.1.
  Substituting Eq. 2 back into Eq. 1 gives                                                       3.3.1 Governing equations. We start with the Eulerian governing
                                                                                               equations:
                      ξ i (x)PT (z − x)M−1 (x)P(xi − x)ui ,
                  X
          u (z) =                                               (3)
                                                                                                           Dρ
                      i ∈B x                                                                                  + ρ∇ · v = 0 (conservation of mass),                              (5)
                                                                                                           Dt
which can also be expressed as u (z) = i ∈Bx Φi (z)ui , where Φi (z) =                                     Dv
                                                 P

ξ i (x)PT (z − x)M−1 (x)P(xi − x) can be defined as the nodal shape                                      ρ    = ∇ · σ + ρg (conservation of momentum),                          (6)
                                                                                                           Dt
function of xi in EFG. Interestingly this is exactly the shape function
used in the Reproducing Kernel Particle Method (RKPM) [Liu et al.                              where ρ is mass density, v is velocity, g = (0, −9.8, 0)T is gravity,
                                                                                                                        Dϕ        ∂ϕ
1995].                                                                                         σ is Cauchy stress, and Dt = ∂t + v · ∇ϕ denotes the material
     The polynomial subspace is usually composed of monomials up to                            derivative of any function ϕ (x, t ).
degree m. In 2D this corresponds to P(x) = [1]T for the constant ba-
                                                                                                  3.3.2 Weak form. As in standard Finite Element Methods [Hughes
sis, P(x) = [1, x, y]T for linear basis, and P(x) = [1, x, y, xy, x 2 , y 2 ]
                                                                                               2012], the weak formulation of the governing PDE involves multi-
for quadratic basis. Since the constant function is always a basis,
                                                                                               plying the differential equation by a test function, integrating by
we automatically have partition of unity, i.e. i Φi (x) = 1. With
                                                      P
                                                                                               parts, and applying boundary conditions.
a complete degree-m polynomial basis (l = m), MLS is m-order-                                                                                                    n
                                                                                                  Denoting the material domain at time t n with Ωt , an updated
consistent† and reproduces all polynomials in P [Huerta et al. 2004].
                                                                                               Lagrangian time discretization of the weak form of Eq. 6 following
Additionally, if ξ i (x) is of class C k , then Φi (x) is of C min(k,m) .
                                                                                               [Jiang et al. 2016] leads to (we drop g here for simplicity)
   3.1.1 The case of a linear polynomial basis. In the simple case                                  1
                                                                                                       Z
                                                                                                              ρ (x, t n ) v̂ αn+1 (x) − v αn (x) q α (x, t n )dx
                                                                                                                                               
of a complete linear polynomial basis (m = l = 1) as done by
                                                                                                   ∆t Ωt n
Müller et al.[2004] for meshless solids, MLS is a 1st -order-consistent                            Z                                       Z
                                                                                                                     n            n
interpolation scheme for scattered data and derivatives. With P(xi −                             =       n
                                                                                                           q α (x, t   )T α (x, t   )ds −         q (x, t n )σα β (x, t n )dx,
                                                                                                                                               n α, β
x) = [1, (xi − x)T ]T , Eq. 2 gives the reconstructed function value                                  ∂Ωt                                   Ωt
                                                                                                                                                                                (7)
and its gradient estimation at x:
                                               u 1                                                             tn
                                                                                               where q(·, t ) : Ω → Rd is an arbitrary vector-valued test function
                       û                                                                      that vanishes at the Dirichlet boundary ∂Ω D , d = 2 or 3 is the
                      " #                       . 
                           = M−1 (x)QT Ξ(x)  ..  ,            (4)
                       ∇û                                                                     problem dimension, T (x, t ) is the traction field along the boundary.
                                                 u                                             Here we have used vn to denote the current Eulerian velocity at time
                                                
                                                 N
where N is the total number of sample data points, Ξ(x) is the                                 n. v̂n+1 denotes the updated velocity field after forces are applied.
                                                                                                                                            n        n
diagonal weighting matrix with Ξii = ξ i (x), and Q(x) = [P(x1 −                               Both vn and v̂n+1 are defined for x ∈ Ωt as Ωt → Rd . Notice
                                                                                                                                n+1  instead of v , since vn+1 is
                                                                                                                                                   n+1
x), . . . , P(xN − x)]T , M(x) = QT ΞQ.                                                        that we choose the notation v̂
                                                                                                                                                       n+1
                                                                                               only defined on the domain of the next time step Ωt . The weak
† If the approximation reproduces exactly a basis of the polynomials of degree less than       form is also expressed using index notations, where v αn , q α , Tα are
or equal to m , then the approximation is said to have m -order consistency [Huerta                                                     ∂q
et al. 2004].                                                                                  α components of vn , q, T and q α, β = ∂xα . Implicit summation on
                                                                                                                                                   β



                                                                                           ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
146:6 •      Y. Hu et al.




                                                 Fig. 7. We dissect an initially stretched elastic armadillo with two progressive cuts.

                                                                                                                                    n
α, β = 1, . . . , d is also assumed, where d is the problem dimension                           In each integral over Ωpt since x is near xpn , we can approximate
(2 or 3).                                                                                       the continuous equations with nodal data samples
   Typically in FEM, both the unknown variable and the test function
                                                                                                                      v αn (x) =   Φj (x)v njα
                                                                                                                                 X
                                                                                                                                                               (8)
can be approximated by functions in a finite-dimensional function
                                                                                                                                                 j
space as linear combinations of some basis shape functions. The
main difference between traditional MPM and MLS-MPM is the                                      and
                                                                                                                               q α (x, t n ) =                  n
                                                                                                                                                 X
choice of this function space. Traditional MPM uses B-spline basis                                                                                       Φi (x)qiα ,                       (9)
functions while MLS-MPM uses MLS shape functions (Φi (x) in §3.1).                                                                                   i
This choice is the key contribution of MLS-MPM and will be shown
                                                                                               where we used the MLS shape function (§3.1)
to provide important advantages.
                                                                                                              Φi (x) = ξ i (xpn )PT (x − xpn )M−1 (xpn )P(xi − xpn ).                     (10)
   3.3.3 Traditional MPM discretization. MPM discretizes all spa-                               Therefore
tial terms using B-spline grid basis functions Ni (x) as (with im-                                  XZ
plicit summation): q α (x, t n ) = Ni (x)qiα
                                          n , v n (x) = N (x)v n , and                                               ρ (x, t n )v αn (x)q α (x, t n )dx =                n n
                                                                                                                                                                X
                                                α        j     jα                                                n
                                                                                                                                                                        qiα v jα mi j ,   (11)
                                                                                                      p       Ωpt
v̂ αn+1 (x, t n ) = N j (x)v̂ n+1 . This further induces the lumped mass for-                                                                                  p,i, j
                     P jα
mulation mni = p Ni (xpn )mp (see [Jiang et al. 2016] for a detailed                           where mi j = Ωt n ρ (x, t n )Φi (x)Φj (x)dx is the mass matrix. Mass
                                                                                                            R
                                                                                                               p
derivation).                                                                                   lumping further approximates it with a diagonal matrix by summing
                                                                                               each row. The diagonal entry is
   3.3.4 MLS-MPM momentum term. In this section we show how                                          XZ
                                                                                               mni =           ρ (x, t n )Φi (x)dx ≈    mp Φi (xpn ) =   mp Ni (xpn ),
                                                                                                                                     X                 X
MLS-MPM discretizes the left hand side of the weak form Eq. 7. We
                                                              n                                             n
first divide the continuum domain with particle partitions Ωpt as                                         p     Ωpt                                      p                     p

                      Z                                                                        which is consistent with traditional MPM. See [Jiang et al. 2016] for
                                                                                               further derivations which are applicable to MLS-MPM as well.
                                   ρ (x, t n )v αn (x)q α (x, t n )dx
                          Ωt
                               n                                                                  The grid velocity is evolved from vni to v̂n+1
                                                                                                                                              i . Intuitively, we can
                      XZ                                                                       approximate time t n+1 velocities around time t n particle locations
                  =                      ρ (x, t n )v αn (x)q α (x, t n )dx.
                                     n
                                   Ωpt                                                         xpn using MLS expression v̂ αn+1 (x) = PT (x − xpn )cv̂ αn+1 (xpn ), where
                      p
                                                                                               the subscript in c denotes the reconstructed physical quantity. As ex-
                                                                                               plained in §3.2, evaluating cv̂ αn+1 corresponds to the grid-to-particle
                                                                                               transfer in APIC/PolyPIC.
                                                                                                   3.3.5 MLS-MPM stress term. The key contribution of MLS-MPM
                                                                                                is on the stress term, i.e., the right hand side of Eq. 7 without the
                                                                                                boundary traction term. Note that the boundary integral evaluates
                                                                                                to 0 assuming a zero Neumann boundary condition (no prescribed
                                                                                                traction at the boundary).
                                                                                                  Choose the test function. Similarly to the momentum term, we can
                                                                                                express the stress integral through the summation over individual
                                                                                                particle domains:
                                                                                                                   Z
                                                                                                                −        q (x, t n )σα β (x, t n )dx
                                                                                                                        n α, β
                                                                                                                     Ωt
                                                                                                                   XZ
                                                                                                              =−             q (x, t n )σα β (x, t n )dx.
                                                                                                                           n α, β
                                                                                                                                                              (12)
                                                                                                                           p       Ωpt
Fig. 8. An elastoplastic von-Mises Jello block is two-way coupled with rigid
                                                                                                Recall in Eq. 9, we have chosen to express the test function q(x, t )
blocks with different density ratios.
                                                                                                from a finite-dimensional function space (the discretized test space).

ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
            A Moving Least Squares Material Point Method with Displacement Discontinuity and Two-Way Rigid Body Coupling • 146:7


This allows us to apply standard Finite Element procedures [Hughes
2012] to convert Eq. 12 into a system of equations by letting q be,
in turn, each of the basis functions in the test space. The resulting
system contains Nд d equations for all the degrees of freedom, where
Nд is the total number of grid nodes and d is the problem dimension.
For the degree of freedom corresponding to any grid node iˆ ∈
{1, . . . , Nд } and component α̂ ∈ {1, . . . , d}, we can enforce such a
choice of q by setting

            n                   1 if α = α̂ and i = iˆ
                               
           qiα = δi iˆδ α α̂ = 
                                0 otherwise
                               
in Eq. 9. Combining this with the MLS shape function from Eq. 10
leads to
                                                                                              Fig. 9. We stir a bowl of dry sand with two thin plates. Materials from
      q α (x, t n ) = PT (x − xpn )M−1 (xpn )ξ iˆ (xpn )P(xiˆ − xpn )δ α α̂       (13)        opposite sides experience independent dynamics even with the use of a
                   n                                                                          single shared background grid.
for any x ∈ Ωpt . Such test functions will be enumerated over all iˆ
and α̂ to get the resulting force components fiˆα̂ associated with all
degrees of freedom on the grid.
                                                                                                          n+1                                           n+1
                                                                                                                 
                                                                                               I + ∆t ∂v̂∂x (xpn ) Fpn , where traditional MPM uses ∂v̂∂x (xpn ) =
  Discretizing the force. To reach the discrete force, Eq. 12 requires
                                                                                              P n+1           n T
the derivative of q. Differentiating Eq. 13 gives                                               i v̂i ∇Ni (xp ) . In the MLS view of v(x) we can differentiate
                                                                                              Eq. 8. For linear polynomials this leads to
                       ∂PT (x − xpn )
  q α, β (x, t n ) =                    M−1 (xpn )ξ iˆ (xpn )P(xiˆ − xpn )δ α α̂ . (14)
                           ∂x β                                                                           ∂ v̂n+1
                                                                                                                  = Cpn+1       and Fpn+1 = I + ∆tCpn+1 Fpn ,
                                                                                                                                                      
                                                                                                                                                                              (17)
To simplify the derivation, we adopt the linear polynomial space                                             ∂x
PT (x − xpn ) = [1, x 1 − xp1
                           n , x − x n , x − x n ]. Note that it is possible
                                2   p2 3      p3                                              where Cpn+1 is exactly the affine velocity matrix from APIC. Ac-
to generalize this choice to a higher order polynomial space, and                             cordingly if we assume hyperelasticity with total potential energy
we leave such an extension to future work. We also choose ξ iˆ = Niˆ                          E = p Vp0 Ψp (Fp ) where Vp0 is particle initial volume and Ψp is the
                                                                                                  P
to be quadratic/cubic B-splines (so that M−1 is a constant). Under                            energy density function, it can be shown that
these assumptions, Eq. 14 becomes
                                                                                                             ∂E                         ∂Ψ n n T n
               q α, β (x, t n ) = Mp−1 Ni (xpn )(x iˆβ − xp β )δ α α̂ ,                                               Ni (xpn )Vp0 Mp−1   (F )F (xi − xpn ),
                                                                                                                    X
                                                                                  (15)              fi = −       =−                                                           (18)
                                                                                                             ∂xi    p
                                                                                                                                        ∂F p p
where Mp = 14 ∆x 2 for quadratic Ni (x) and 31 ∆x 2 for cubic Ni (x).
  Substituting it back into Eq. 12 reveals the α̂ component force                             which is consistent with the weak form result from Eq. 16 by noticing
                                                                                                      1 ∂Ψ FT and det(F)V 0 = V n .
                                                                                              σ = det(F
computation on grid node iˆ:                                                                            ) ∂F                 p    p
                     XZ                                                                          In contrast to traditional MPM, the MLS-MPM deformation gra-
           fiˆα̂ = −         q (x, t n )σα β (x, t n )dx
                           n α, β                                                             dient update and force computation directly re-use quantities from
                           p      Ωpt
                                                                                              APIC and do not require any evaluation of the interpolation function
                               Vpn Mp−1 σp nα̂ β Niˆ (xpn )(x nˆ − xpnβ ),
                         X
                   ≈−                                                             (16)        gradient throughout the algorithm. This greatly simplifies the imple-
                                                            iβ
                           p                                                                  mentation of MPM and substantially decreases the computational
                                                                                              cost (see §6 for more details).
where Vpn is the current volume of particle p at time n. Here the
approximation comes from adopting a one-point quadrature rule to
                            n
replace σ (x, t n ) in Ωpt with σ pn .
                                                                                              3.5     Implicit Integration
   Note that in contrast to our result, traditional MPM uses fiˆα̂ =                          As in [Stomakhin et al. 2013], implicit time stepping is naturally sup-
− p Vpn σp nα̂ β Niˆ, β (xpn ) which requires explicitly differentiating the                  ported by MLS-MPM. Implicit MPM with Newton’s method [Gast
  P

interpolation function Niˆ (x).                                                               et al. 2015] requires computing the action of the energy Hessian
                                                                                              on an arbitrary grid increment δ u. We show in the supplementary
3.4    Deformation gradient and force                                                         document [Hu et al. 2018] that

Deformation gradient F = ∂Z                                                                                           Vp0 Ap Fpn T Mp−1 Ni (xpn )(xni − xpn ),
                                                                                                                   X
                            ∂X is usually used to characterize finite                                      −δ fi =                                               (19)
deformation in elastoplasticity, where X denotes the material space,                                                    p
and Z(X, t ) is the deformation map. In MPM, F is evolved on each
                         ∂ F(X, t ) = ∂v (Z(X, t ), t )F(X, t ), where                                       ∂2 Ψ : P M −1 N (xn )δ u (xn − xn )T Fn . In practice
material particle with ∂t                ∂x                                                   where Ap = ∂F∂F         j p      j p      j j     p    p
Eulerian velocity gradient ∂v
                            ∂x   is discretized on the grid. Based on                         it corresponds to a grid-to-particle gather step (for computing Ap )
the updated Lagrangian view, particle-wise Fp is updated as Fpn+1 =                           and a particle-to-grid scatter step (for accumulating δ fi ).

                                                                                          ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
146:8 •      Y. Hu et al.


                                                                                            5     METHOD: MLS-MPM WITH CPIC
                                                                                            Here we detail the steps from time t n to t n+1 for MLS-MPM, en-
                                                                                            hanced with a Compatible Particle-In-Cell (CPIC) algorithm for
                                                                                            material discontinuity and rigid-body coupling (see Fig. 10 for a
                                                                                            diagram of the logic steps). Note that we use the term “rigid body”
                                                                                            to denote either a dynamic rigid body or a rigid collision bound-
                                                                                            ary with scripted kinematics motion. We use p, q for MPM particle
                                                                                            indices, r, s for rigid body indices, and i, j for grid node indices.

                                                                                            5.1    Rigid-rigid collision
                                                                                            This step includes rigid-rigid collision detection/resolution and rigid
                                                                                            body articulation. It is independent from our MPM algorithm, and
                                                                                            any external rigid body dynamics package can be used. We will skip
                                                                                            the details and denote the updated velocity and angular velocity of
                                                                                            rigid body r as vr∗ ← vnr and ωr∗ ← ωrn . Note that vr∗ and ωr∗ are still
                                                                                            intermediate rigid body velocities. They will be further updated to
                                                                                            vn+1
                                                                                             r    and ωrn+1 at the end of time step n (see §5.6).

Fig. 10. Algorithm overview from time t n to t n+1 for MLS-MPM with CPIC.                   5.2    Splat grid-wise colored distance field (CDF)
Steps: (1) Rigid-rigid collision and rigid body articulation update rigid body              Traditional signed distance functions (SDFs) are convenient for
velocities (§5.1); (2) Splat rigid body to grid CDF (§5.2); (3) Reconstruct                 performing inside/outside queries and normal estimations. As such,
particle CDF from grid CDF (§5.3); (4) CPIC particle-to-grid transfer and
                                                                                            SDFs are widely used as the implicit surface representation for
rigid body impulses (§5.4); (5) CPIC grid-to-particle transfer (§5.5); (6) MPM
                                                                                            volumetric collision geometries in both FEM [Irving et al. 2004]
particle advection (§5.5); (7) Rigid body advection (§5.6).
                                                                                            and MPM [Stomakhin et al. 2013]. Traditional SDF level sets such
                                                                                            as OpenVDB [Museth 2013] can be easily constructed from closed
                                                                                            surfaces. Lossaso et al. [2006] developed an algorithm for treating
                                                                                            the interface of multiple level sets.
4    FROM MPM TO MLS-MPM                                                                       To represent intersecting open boundaries, we extend discrete
                                                                                            SDFs to Colored Distance Fields (CDFs) with unsigned distance d (xi )
Before introducing additional considerations for material discontinu-
                                                                                            and color information. The color at each point encodes both the set
ity, here we summarize the essential steps in MLS-MPM, since it can
                                                                                            of nearby surfaces and which side xi locates at. As a result CDFs can
be independently used for modifying any existing MPM framework.
                                                                                            discretely represent multiple regions using a single Cartesian lattice.
     (1) Particles to grid. Use APIC [Jiang et al. 2015] or PolyPIC [Fu                     Note that for sub-grid accuracy and more non-trivial topology such
         et al. 2017] to transfer mass and momentum from the parti-                         as non-manifold bifurcation, it is a better choice to construct the
         cles to the grid.                                                                  distance field using the algorithm by Xu and Barbič [2014], or use
     (2) Update grid momentum. Use either symplectic Euler                                  the non-manifold level set proposed by Mitchell et al. [2015] which
         (with force given by Eq. 18) or backward Euler (with force                         stores SDF on a hexahedral mesh.
         differential given by Eq. 19) to update grid momentum.
     (3) Grid to particles. Use APIC or PolyPIC to transfer veloci-
         ties and affine/polynomial coefficients from the grid to the
         particles.
     (4) Particle deformation gradient. Update particle deforma-
         tion gradient using the MLS approximation to the velocity
         gradient (Eq. 17).
     (5) Update particle plasticity. Project particle deformation
         gradient for plasticity (if there is any).
     (6) Particle advection. Particle positions are updated with
         their new velocities.
                                                                                            Fig. 11. Splatting the unsigned distance field from a rigid particle on a
The only differences between MLS-MPM and traditional MPM are
                                                                                            segment to 9 grid nodes. The u axis is the normal to the plane defined by
the force expression in step (2) and the F update in step (4). In fact,                     the primitive. The value of u i,r η for each grid node thus represents the
step (4) in MLS-MPM is simpler than MPM due to the reuse of                                 signed point-plane distance between grid node i and the plane that rigid
Cpn+1 constructed in step (3). Step (2) in MLS-MPM is also easier to                        particle r η lies on. Note that such a distance is only considered to be valid
implement than MPM, and allows an easy-to-achieve performance                               (or existing) if the projection onto the plane actually lies inside the primitive
gain as discussed in §6.                                                                    geometry.


ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
           A Moving Least Squares Material Point Method with Displacement Discontinuity and Two-Way Rigid Body Coupling • 146:9




Fig. 12. (a) Three intersecting thin rigid boundaries; (b) Grid unsigned distance field; (c) Grid colors (relationship to boundaries); (d) Maintained particle color;
(e)(f) Particle distances to the boundary and the normals reconstructed with MLS.


   5.2.1 Grid unsigned distance. In this section we describe our fast                 lattice of particles on each triangle so that the minimum particle
algorithm for constructing a narrow-band unsigned distance field                      distance is smaller than grid spacing ∆x.
on the grid from rigid body surfaces.
                                                                                          5.2.2 Grid color field. The color of each grid node contains its
   Rigid particles. For each oriented rigid surface r , we adaptively                 affinity (closeness) to each rigid surface and a tag labeling the side
sample auxiliary rigid particles on the mesh. We index these points                   it is on. Affinity Air for surface r and grid node i is
as r 1 , r 2 , . . . , r R , where R is the total number of rigid particles on
                                                                                                                    1, ∃η with valid ui,r η ,
surface r . We also use E (r η ) to denote the primitive (segment in 2D
                                                                                                                   
                                                                                                           Air =   0, otherwise.                         (20)
and triangle in 3D) index for rigid particle r η .                                                                 
   Valid distance. For computing a narrow-band distance field, we                     Note that the validity of the signed distance ui,r η between grid node
allow each rigid particle r η to influence the closest grid node and the              i and rigid particle r η is defined in §5.2.1. The tag Tir is determined
surrounding 3 × 3 × 3 grid nodes in 3D. We can quickly project these                  by the signed distance of the closest rigid particle r η ∗ (xi ) , i.e.,
27 grid nodes onto the plane defined by E (r η ) and calculate the                    Tir = sign(ui,r η ∗ (xi ) ).
signed point-plane distance ui,r η determined by grid node xi and                        5.2.3 Efficient CDF storage. Ideally one would like to have one
rigid particle r η . We illustrate this operation in Fig. 11. For efficiency,         CDF for each rigid body, but this is expensive in both computation
the distance is only considered valid and stored if the projection                    and storage. Therefore, we store only one unsigned distance (32-bit
point lies inside the primitive (i.e. when point-plane distance equals                float) and an extra 32-bit encoding of Air ,Tir (2 bits for each rigid
point-primitive distance).                                                            body). This allows us to compress the CDF into 64 bits per grid node.
   Distance rasterization. The minimum unsigned distance from xi
to the boundary is then
                                                                                      5.3    Reconstruct particle-wise colored distance field
                                                                                      Once we have the grid CDF (di , Air and Tir ), they can be recon-
                             di = min |ui,r η |.
                                    r,η                                               structed at other locations near the rigid surfaces. In our case we
During the process of computing all point-plane distances, we also                    look at MPM particle locations xp . In Fig. 12 we show the recon-
keep track of the closest rigid body to xi using index r ∗ (xi ). This                struction result for three intersecting rigid boundaries in 2D.
index will be used in §5.4 for determining which rigid body we apply                     5.3.1 Particle color field. The color information can be recon-
impulses on from grid velocities. Since each rigid body contains                      structed relatively easily. Specifically, a particle’s affinities to rigid
many rigid particles, we also track the rigid particle index r η ∗ (xi )              surfaces Apr are directly inherited from grid affinity Air , where
that results in the smallest point-plane distance for node i and rigid                grid node i is any node within particle p’s MPM support kernel. For
body r . This index will be used in §5.2.2 for uniquely deciding the                  particle tag Tpr , we take a distance weighted average
relative side relationship between them.
                                                                                                                    X
   Trade-off. While there will be missing values at certain corners                                     Tpr = sign * Ni (xp )di Tir + ,             (21)
(which will be robustly handled as discussed in §5.3.2), this splat-                                               , i              -
ting process from rigid particles to grid nodes provides a very fast                  where incorporating di in the weight reduces the influence of grid
construction of a narrow band unsigned distance field with only                       nodes that are near the rigid body whose tags can be ambiguous
point-plane projection computations. This process requires much                       due to floating point error.
less computation compared to the exact distance evaluation between
                                                                                         5.3.2 Particles distance and normal. As a particle approaches the
points and meshes.
                                                                                      surface boundary, it is not guaranteed to have a complete set of
  Rigid particle seeding. Note that our algorithm is not sensitive to                 grid CDFs in its entire kernel support. Our fast distance splatting
the distribution of the rigid particles, as long as on each triangle                  algorithm (§5.2.1) also tends to miss a small number of nodes near
there is at least one particle, and the whole triangle is covered by                  mesh corners. Therefore we cannot directly interpolate di to the
the kernel range of the particles. Specifically, we uniformly seed a                  particles. Instead, we use the robust MLS reconstruction technique

                                                                                  ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
146:10 •       Y. Hu et al.


described in §3.1. According to the tag information on the particle
and its associated grid nodes, we locally convert the grid unsigned
distances to signed distances. Then we adopt Eq. 4 to reconstruct
particle distance dp and its gradient ∇dp , where particle normal np
is given by np = ∇dp /|∇dp |.
    5.3.3 Particle color persistence and penalty force. Particle p’s
color Apr and Tpr associated with rigid surface r will persist after
being obtained, until all nodes in p’s kernel lose affinities with r . This
is important since a particle may slightly penetrate a surface due
to numerical advection error, in which case we should not flip the
tag. When this happens (see Fig. 13), we will then get a negative dp
with a correct normal np along which we could fix the penetration.
Specifically, we detect negative dp occurrences and keep track of a
weak penalty force on these particles as                                                    Fig. 14. A vertical Drucker-Prager plastic sand flow is two-way coupled
                                                                                            with four rigid paddles, revealing intricate dynamics and flow patterns.
                                 fpP,n = −kh dp np ,                           (22)
where kh is the penalty stiffness parameter.                                                We also define a boundary projection operator for projecting an
   5.3.4 Particle grid compatibility. The reconstructed color infor-                        input velocity v given normal n and boundary condition B (sticky,
mation immediately allows us to partition all grid nodes within a                           slip, or separate):
particle’s kernel. We use Si to denote the set of surfaces that has                                                   ~0,        B is sticky,
non-zero Air and Sp for that of particle p. A grid node i and a particle
                                                                                                                     
                                                                                                                     
                                                                                                                     
                                                                                                                      vt ,      B is slip,
                                                                                                                     
                                                                                                                     
p are compatible if and only if for all surfaces shared by the particle
                                                                                                                     
                                                                                                  Proj(v, n, B, µ) =                                              (25)
and the grid node, all tags are the same (Tir = Tpr , ∀r ∈ Si ∩ Sp ).
                                                                                                                     
                                                                                                                     
                                                                                                                     
                                                                                                                      ζ v̂t ,   B is separate and v · n ≤ 0,
                                                                                                                                 B is separate and v · n > 0,
                                                                                                                     
                                                                                                                      v,
                                                                                                                     
5.4    CPIC particle-to-grid transfer
                                                                                            where ζ = max(0, |vt | + µv · n), vt = v − (v · n)n, v̂t = |vvt | . Here
We use i p+ to denote nodes that are compatible with particle p,                                                                                          t
                                                                                            µ ≥ 0 is the dynamic friction coefficient.
and i p− for the incompatible nodes. Similarly, p i+ are the particles
that are compatible with grid node i, and p i− are the incompatible                            Velocity projection and impulses on rigid bodies. Given a particle
particles.                                                                                  p and rigid body r , the velocity contribution to the incompatible
   We will assume the usage of APIC, although the extension to                              grid node i is projected to Projr (vpn , np , xi ) = Vnr (xi ) + Proj(vpn −
PolyPIC is straightforward. Near rigid surfaces, particles only trans-                      Vnr (xi ), np , Br , µ r ), where Br and µ r are the boundary type and fric-
fer to compatible grid nodes:                                                               tion coefficient of rigid body r . In CPIC, each incompatible grid node
                                                                                            j ∈ i p− results in an impulse mp (vpn − Projr ∗ (vpn , np , xj ))N j (xpn )
             mni =        Ni (xqn )mq ,
                     X
                                                                  (23)
                                                                                            applied to the closest rigid body r ∗ (xj ) (tracked in §5.2.1) at xj .
                       q ∈ {p i + }

           (mv)in =                   Ni (xqn )mq vqn + Cqn (xi − xqn ) .                     MLS-MPM grid momentum update. The grid momentum is up-
                          X                                           
                                                                               (24)
                                                                                            dated as (assuming symplectic Euler)
                       q ∈ {p i + }
                                                                                                           (m v̂)in+1 = (mv)in + ∆t mni g + fin ,
                                                                                                                                              
   Velocity projection operator. For each rigid body, we can compute                                                                              (26)
its world-space velocity at position x as Vnr (x) = vnr + ωrn × (x − xnr ).                 where g is gravity and fin is the MLS-MPM hyperelastic force given
                                                                                            by Eq. 18. Note that we use notation (m v̂)in+1 instead of (mv)in+1
                                                                                            since the later refers to the grid momentum in time t n+1 transferred
                                                                                            from the particles. Implicit discretization of the hyperelastic force
                                                                                            can be achieved similarly to [Stomakhin et al. 2013]. Then we up-
                                                                                            date the velocities by dividing the momentum by mass: v̂n+1    i    =
                                                                                            (m v̂)in+1 /mni .

                                                                                            5.5    CPIC grid-to-particle transfer
                                                                                            Normally for level set-based collision objects, boundary conditions
                                                                                            are applied at grid nodes inside the level set. In our case for each
Fig. 13. Here we show the motion of a particle slightly penetrating a bound-                particle, the velocities on incompatible grid nodes are however non-
ary due to numerical advection error. In this case, our method robustly                     associated with the particle due to the enforcement of discontinuity.
maintains a persistent particle color (Apr , Tpr ) and normal np . The recon-               We take a ghost velocity approach, where we assume for any node
structed distance becomes negative when penetration happens. This allows                    j ∈ i p− , its velocity is simply vj = vpn through a constant extrapola-
us to apply a weak penalty force as explained in §5.3.3.                                    tion from particle p. Thus the CPIC transfer from grid to particle

ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
            A Moving Least Squares Material Point Method with Displacement Discontinuity and Two-Way Rigid Body Coupling •                                                            146:11


which gathers contributions from both incompatible and compatible                                          Timing (ms)         Reference   Ours (MPM)     Ours∗ (MPM)     Ours∗ (MLS-MPM)

nodes is given by                                                                                          P2G (1 thread)      4760 (1×)   5744 (0.83×)    2685 (1.77×)        1283 (3.71×)
                                                                                                           P2G (4 threads)     1220 (1×)   1525 (0.80×)     688 (1.77×)         328 (3.72×)
   vpn+1 =      N j (xpn ) ṽp +   N j (xpn ) v̂n+1
            X                    X
                                                j ,          (27)                                          G2P (1 thread)      8255 (1×)   7476 (1.10×)    1144 (7.21×)        589 (14.01×)
                                                                                                           G2P (4 threads)     2070 (1×)   2011 (1.03×)     313 (6.61×)        163 (12.70×)
               j ∈i p−                   j ∈i p+
                                                                                                       Table 2. Benchmarks of MPM transfer operations. Reliable reference im-
                                                                                                       plementation is from [Tampubolon et al. 2017]. Superscript ∗ is with our
    Cpn+1 = Dp−1 *.             N j (xpn ) ṽp znjp T +             N j (xpn ) v̂n+1 n T+
                         X                                 X
                                                                                 j z jp / ,            performance optimization. All performance data are collected on an PC
                     ,j ∈i p−                             j ∈i p+                       -              with an Intel Core i7-7700K CPU with four cores at 4.2GHz, and 4 × 8 GB
                                                                                            (28)       DDR4 memory at 2400 MHz. Intel Turbo Boost is disabled for stable CPU
where znjp = xj − xpn . Here we include ghost velocities on incompat-                                  frequency.
ible nodes to prevent potential singularity of Dp .
   The collided particle velocity is given by ṽp = Projr (vpn , npn , xj ) +
∆tcnp , where r denotes the closest rigid boundary to the particle and
c is non-zero when there is need to push the particle away from the
boundary. Particle advection is then given by xpn+1 = xpn + ∆tvpn+1 .                                  eight-colored P2G for lock-free multi-threading, as detailed in the
                                                                                                       supplementary document [Hu et al. 2018].
With MLS-MPM, particle deformation gradient is updated as Fpn+1 =
(I + ∆tCpn+1 )Fpn . Then we apply the penalty impulse ∆tfpP,n (see                                       Algorithmic improvement. MLS-MPM halves the number of FLOPs
Eq. 22) to the particle and the reverse impulse to the rigid body for                                  needed for each particle. The unification of affine velocity field
conservation of momentum.                                                                              and deformation gradient eliminates the necessity for evaluating
                                                                                                       ∇Ni (xpn ), which speeds up both P2G and G2P. During P2G, MLS-
5.6      Rigid body advection                                                                          MPM fuses the scattering of the affine momentum and particle force
Dynamic rigid body velocities can be updated from the impulses                                         contribution into Ni (xpn )Qp (xi − xpn ), where
computed in §5.4: vn+1
                    r  ← vr∗ and ωrn+1 ← ωr∗ , where vr∗ and ωr∗ are
evolved from vnr and ωrn as described in §5.1. Then the rigid bodies                                                                            ∂Ψ n n T
                                                                                                                             Qp = ∆tVp0 Mp−1      (F )F + mp Cp ,
are advected in a standard way.                                                                                                                 ∂F p p

6     A HIGH PERFORMANCE IMPLEMENTATION                                                                so that only one matrix-vector multiplication is needed for the inner
Efficiency is a key concern in MPM since simulating a large number                                     loop (27 iterations for 3D and quadratic B-spline); and during G2P,
of particles can be time-consuming. The transfer operation from                                        it avoids evaluating ∂v
                                                                                                                             ∂x with ∇Ni (x).
particle to grid (P2G) and that from grid to particle (G2P) are the
bottlenecks for traditional MPM, which usually takes more than 85%                                     6.1    Benchmark and discussion
of time based on our experience. In this section, we discuss our high-                                 We optimize the code for both traditional MPM and MLS-MPM.
performance implementation of these two operations. Particularly,                                      We also examine the generated assembly code to ensure that the
we decompose the performance gain into two parts: performance                                          compiler (gcc 5.4.1) correctly translates ideas mentioned above into
engineering and algorithmic improvement.                                                               machine instructions. Note that MLS-MPM is also easier to optimize
  Performance engineering. We adopt low-level performance opti-                                        thanks to its simplicity. To evaluate our implementation, we set up a
mization techniques to accelerate the program with no algorith-                                        benchmark with 8×106 uniformly distributed particles, and measure
mic change. We use SPGrid [Setaluri et al. 2014] for background                                        P2G and G2P time consumption for different implementations. More
grid storage, and adopt techniques including blocked transfer and                                      details on the benchmark is given in the supplementary document
                                                                                                       [Hu et al. 2018]. Results are summarized in Table 2, showing we
                                                                                                       achieve 3.71× and 14.01× higher performance for P2G and G2P
                                                                                                       respectively compared to a reliable implementation of the MPM
                                                                                                       solver in [Tampubolon et al. 2017] (with OpenVDB [Museth 2013]).
                                                                                                          To further validate this significant improvement, we measure
                                                                                                       the floating point unit (FPU) utilization using Intel VTune on our
                                                                                                       optimized MLS-MPM and [Tampubolon et al. 2017]. Our P2G and
                                                                                                       G2P implementations lead to 1.93× and 7.38× FPU utilization com-
                                                                                                       pared with [Tampubolon et al. 2017]. Note that the performance
                                                                                                       improvement of our optimized traditional MPM over [Tampubolon
                                                                                                       et al. 2017] is proportional to the gain in FPU utilization.
                                                                                                          Notably, the usage of MLS-MPM directly enhances MPM perfor-
                                                                                                       mance by 2× in the case of explicit time integration, regardless of
                                                                                                       whether low-level performance engineering is performed. Consid-
                                                                                                       ering its ease of implementation, MLS-MPM can be easily imported
      Fig. 15. We sweep a pile of sand with a kinematic thin shell object.                             to any existing MPM solvers with APIC/PolyPIC transfers.

                                                                                                   ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
146:12 •       Y. Hu et al.




Fig. 16. Our rigid-MPM coupling allows us to simulate terradynamics to predict legged robot’s locomotion on granular media. We 3D print the model from [Li
et al. 2013], equip it with motors, and demonstrate how our simulation results (top) match real world footage (bottom) on different motion patterns.


   For implicit integration, efficient Krylov-solver-based implicit
MPM usually adopts a matrix-free implementation to avoid recon-                                                   MLS-MPM                                  MLS-MPM
                                                                                                2.5               traditional MPM        0.8               traditional MPM
structing a sparse matrix in every time step. Each Krylov multipli-
                                                                                                2.0
cation is essentially equivalent to a grid-to-particle-to-grid transfer                                                                  0.6
cycle for velocity differentials. Transfers therefore remain the bot-                           1.5
                                                                                            E




                                                                                                                                     E
tleneck. Similarly to the explicit force, our force differential also                                                                    0.4
                                                                                                1.0
eliminates the necessity of evaluating kernel gradients and allows
algorithmic performance gain. The gain is however less significant,                             0.5                                      0.2
because only in explicit time integration it benefits from unifying                             0.0                                      0.0
affine momentum and particle force contribution.                                                      0.0   0.5   1.0    1.5   2.0             0.0   0.2        0.4     0.6
   We developed our system based on Taichi [Hu 2018]. Our high-                                                    t                                        t
performance code will be open-sourced with the publication of this
work. Please refer to our supplementary code and document [Hu                               Fig. 17. Total energy evolution curves for MPM and MLS-MPM in the oscil-
et al. 2018] for more detailed discussion on implementation.                                lating jello (left) and colliding balls (right) test cases. Numerical dissipation
                                                                                            from the two methods are nearly identical.


7    RESULTS
                                                                                            simultaneously demonstrate the robust treatment of infinitely thin
Our experiments show that MLS-MPM produces visually compara-
                                                                                            boundaries and two-way rigid body coupling.
ble dynamics with traditional MPM. We also perform two standard
                                                                                               Two-way coupled rigid-MPM simulation is also useful for robotics
hyperelasticity tests: initially stretched oscillating cube and colliding
                                                                                            and terradynamics. In Fig. 16 we simulate and validate locomotion
balls. The total energy evolution curves for MLS-MPM and MPM are
                                                                                            for robot navigating in granular media. We provide more details
plotted in Fig. 17 showing almost identical numerical dissipation.
                                                                                            about the 3D printed robot in the supplementary document [Hu
   We present various examples to demonstrate the efficacy of MLS-
                                                                                            et al. 2018].
MPM with CPIC. Timing, statistics and material parameters are
                                                                                               CPIC enables powerful new features for MPM at low cost since
given in Table 3. We show world space cutting of elastic and elasto-
                                                                                            only a narrow band near the rigid boundaries needs CPIC. In the
plastic objects including a progressively cut armadillo (Fig. 7), a
                                                                                            banana example (Fig. 1), each frame takes around 131.9s if the cutter
dissected bunny (Fig. 4), a banana (Fig. 1) and a goat cheese block
                                                                                            is removed, while it takes 140.5s when cutting is enabled, with only
(Fig. 3). Similarly, our method handles thin boundary meshes, as
                                                                                            6% CPIC overhead. (For fair comparison in this experiment, CPIC
demonstrated with sweeping (Fig. 15) and stirring (Fig. 9) gran-
                                                                                            and regular MPM transfers are optimized with equal efforts.)
ular materials. Two-way coupling with rigid bodies is naturally
supported, and shown by dropping rigid blocks onto goo (Fig. 8),                            8    LIMITATIONS AND FUTURE WORK
testing buoyancy in water (Fig. 6) and hitting paddles with sand
(Fig. 14). A sand wheel (Fig. 5) and several water wheels (Fig. 1)                          While CPIC tackles infinitely thin boundaries, it only resolves fea-
                                                                                            tures at a scale of grid ∆x. Thus we cannot handle sub-grid level

ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
         A Moving Least Squares Material Point Method with Displacement Discontinuity and Two-Way Rigid Body Coupling •                                                146:13

  Example                      sec/Frame           ∆x         ∆t    Particle #    Density      Young’s Modulus        Bulk Modulus        Yield Stress      Friction Angle
  (Fig. 7) Cutting armadillo        107.0   5.0 × 10−3   5 × 10−6       1.3M           400                1 × 105                     -               -                    -
  (Fig. 4) Bunny split               16.7   3.3 × 10−3   3 × 10−4       2.0M           400              1.5 × 103                     -               -                    -
  (Fig. 1) Water wheel              120.3   2.5 × 10−3   5 × 10−5       1.4M          1000                      -             1 × 104                 -                    -
  (Fig. 6) Buoyancy                 122.9   3.1 × 10−3   1 × 10−4       5.3M          1000                      -             1 × 104                 -                    -
  (Fig. 3) Cheese                   227.1   3.3 × 10−3   2 × 10−5       1.8M           400                4 × 105                   -                10                    -
  (Fig. 8) Goo blocks               105.3   4.2 × 10−3   1 × 10−4       1.8M           400                5 × 104                   -                10                    -
  (Fig. 1) Banana                   140.5   4.2 × 10−3   5 × 10−5       1.2M           400                4 × 105                   -                 5                    -
  (Fig. 5) Sand wheel (2D)            0.3   2.5 × 10−3   1 × 10−4       4.1K          1000              3.5 × 105                   -                 -                   45
  (Fig. 15) Sand sweep              288.3   2.0 × 10−3   2 × 10−5       2.6M           400              3.5 × 105                   -                 -                   35
  (Fig. 9) Sand stir                158.3   3.1 × 10−3   1 × 10−4       5.2M           400              3.5 × 105                   -                 -                   10
  (Fig. 14) Sand paddles             20.0     2 × 10−3   1 × 10−4       1.0M           400              3.5 × 105                   -                 -                   30
  (Fig. 16) Robot                   112.4   3.3 × 10−3   1 × 10−4       3.4M          2000              1.8 × 106                   -                 -                   45
  (Fig. 16) Robot (reverse)         116.8   3.3 × 10−3   1 × 10−4       3.4M          2000              1.8 × 106                   -                 -                   45
Table 3. Particle count and time per frame are provided as average values. All are measured on an Intel Core i7-7700K CPU with four cores at 4.2GHz. We use
the Poisson’s ratio ν = 0.3 for all the examples. Elastic materials are simulated with the fixed Corotated hyperelasticity [Stomakhin et al. 2013]. Weakly
compressible water is done as in [Tampubolon et al. 2017]. Plastic materials adopt St. Venant-Kirchhoff elasticity with von Mises plasticity [Gao et al. 2017].
Granular materials use St. Venant-Kirchhoff elasticity with Drucker-Prager plasticity [Klár et al. 2016].



boundary configurations such as sharp corners and narrow gaps                        gift from Awowd Inc., a gift from NVIDIA Corporation, and a gift
as done by Azevedo et al. [2016] with cut-cells. Our compatibility                   from SideFX.
condition between particles and grids nodes is a binary decision and
essentially grid-aligned. One possible future direction for increasing               REFERENCES
the accuracy would be enforcing a smoother transition region based                   N. Akinci, M. Ihmsen, G. Akinci, B. Solenthaler, and M. Teschner. 2012. Versatile rigid-
on sub-grid features. Also, it is hard to reconstruct sharp cutting sur-                 fluid coupling for incompressible SPH. ACM Trans Graph 31, 4, Article 62 (July
                                                                                         2012), 8 pages.
faces from particles. Incorporating embedded meshes as in [Wojtan                    V. Azevedo, C. Batty, and M. Oliveira. 2016. Preserving geometry and topology for fluid
et al. 2009] would be worth investigating. Fully implicit rigid-MPM                      flows with thin obstacles and narrow gaps. ACM Trans Graph 35, 4 (2016), 97.
strong coupling is not formulated in this work and we leave that as                  S. Band, C. Gissler, and M. Teschner. 2017. Moving least squares boundaries for SPH
                                                                                         fluids. Virtual Reality Interactions and Physical Simulations (VRIPhys) (2017).
future work.                                                                         B. Banerjee, J. Guilkey, T. Harman, J. Schmidt, and P. McMurtry. 2012. Simulation
   MLS-MPM provides a new perspective for discretizing the gov-                          of impact and fragmentation with the material point method. arXiv preprint
erning equations that is consistent with other meshless approaches.                      arXiv:1201.2452 (2012).
                                                                                     Z. Bao, J. Hong, J. Teran, and R. Fedkiw. 2007. Fracturing rigid materials. IEEE Transac-
We believe it also builds a foundation for devising higher order                         tions on Visualization and Computer Graphics 13, 2 (2007).
MPM schemes for enhanced accuracy and visual vividness. It is also                   C. Batty, F. Bertails, and R. Bridson. 2007. A fast variational framework for accurate
                                                                                         solid-fluid coupling. ACM Trans Graph 26, 3 (2007).
a promising direction to look into reducing the cell-crossing error                  M. Becker, H. Tessendorf, and M. Teschner. 2009. Direct forcing for Lagrangian rigid-
when multilinear kernels are used. While traditional MPM readily                         fluid coupling. IEEE Transactions on Visualization and Computer Graphics 15, 3 (May
goes unstable in this case, MLS-MPM provides possible solutions                          2009), 493–503.
                                                                                     T. Belytschko, Y. Lu, and L. Gu. 1994. Element-free Galerkin methods. International
due to its freedom in choosing the function space and weighting                          journal for numerical methods in engineering 37, 2 (1994), 229–256.
functions. Efficient spatial adaptivity with [Gao et al. 2017] would be              T. Belytschko and M. Tabbara. 1996. Dynamic fracture using element-free Galerkin
another interesting topic for further study, since MLS-MPM does not                      methods. Internat. J. Numer. Methods Engrg. 39, 6 (1996), 923–938.
                                                                                     J. Brackbill and H. Ruppel. 1986. FLIP: A method for adaptively zoned, Particle-In-Cell
need a regular grid or intricate discretization strategies on hanging                    calculations of fluid flows in two dimensions. J Comp Phys 65 (1986), 314–343.
nodes. Due to its robust nature, moving least squares would work                     M. Carlson, P. Mucha, and G. Turk. 2004. Rigid fluid: animating the interplay between
                                                                                         rigid bodies and fluid. ACM Trans Graph 23, 3 (2004), 377–384.
on any unstructured (or even non-manifold) grid. For example it                      Z. Chen, M. Yao, R. Feng, and H. Wang. 2014. Physics-inspired adaptive fracture
would be interesting to investigate applying MLS-MPM to power                            refinement. ACM Trans Graph 33, 4 (2014), 113.
diagrams [de Goes et al. 2015] for solids and granular media. Further-               N. Chentanez, T. Goktekin, B. Feldman, and J. O’Brien. 2006. Simultaneous coupling of
                                                                                         fluids and deformable bodies. In Proc ACM SIGGRAPH/Eurograph Symp Comp Anim.
more, coupling MPM particles and mesh-based solvers (e.g. cloth)                         Eurographics Association, 83–89.
with CPIC would be a potential future direction, since the boundary                  G. Daviet and F. Bertails-Descoubes. 2016. A semi-implicit material point method for
particles can represent arbitrary codimension-1 manifolds.                               the continuum simulation of granular materials. ACM Trans Graph 35, 4 (2016),
                                                                                         102:1–102:13.
                                                                                     F. de Goes, C. Wallez, J. Huang, D. Pavlov, and M. Desbrun. 2015. Power particles: an
ACKNOWLEDGMENTS                                                                          incompressible fluid solver based on power diagrams. ACM Trans Graph 34, 4 (2015),
                                                                                         50–1.
We are grateful to the anonymous reviewers for their valuable                        B. Feldman, J. O’Brien, and B. Klingner. 2005. Animating gases with hybrid meshes. In
                                                                                         ACM Trans Graph, Vol. 24. ACM, 904–909.
suggestions and comments. We thank Christopher Long from Los                         S. Fleishman, D. Cohen-Or, and C. Silva. 2005. Robust moving least-squares fitting with
Alamos National Laboratory for useful discussions, Hannah Bol-                           sharp features. In ACM Trans Graph, Vol. 24. ACM, 544–552.
lar from University of Pennsylvania for narrating the video, and                     C. Fu, Q. Guo, T. Gast, C. Jiang, and J. Teran. 2017. A polynomial Particle-In-Cell method.
                                                                                         ACM Trans Graph 36, 6, Article 222 (2017), 12 pages.
Hangxin Liu from UCLA CS Department for assisting the robot                          M. Gao, A. Pradhana Tampubolon, C. Jiang, and E. Sifakis. 2017. An adaptive Generalized
experiments. The work is partially supported by Jiang’s StartUp                          Interpolation Material Point Method for simulating elastoplastic materials. ACM
                                                                                         Trans Graph 36, 6 (2017).
Grant from the University of Pennsylvania, NSF IIS-1755544, Na-                      T. Gast, C. Schroeder, A. Stomakhin, C. Jiang, and J. Teran. 2015. Optimization integrator
tional Key Technology R&D Program of China (2017YFB1002701), a                           for large time steps. IEEE Trans Vis Comp Graph 21, 10 (2015), 1103–1115.


                                                                                 ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.
146:14 •        Y. Hu et al.


E. Guendelman, A. Selle, F. Losasso, and R. Fedkiw. 2005. Coupling water and smoke to         M. Müller, R. Keiser, A. Nealen, M. Pauly, M. Gross, and M. Alexa. 2004. Point based
    thin deformable and rigid shells. ACM Trans Graph 24, 3 (July 2005), 973–981.                 animation of elastic, plastic and melting objects. In Proc ACM SIGGRAPH/Eurograph
D. Hahn and C. Wojtan. 2015. High-resolution brittle fracture simulation with boundary            Symp Comp Anim. Eurographics Association, 141–151.
    elements. ACM Trans Graph 34, 4 (2015), 151.                                              K. Museth. 2013. VDB: High-resolution sparse volumes with dynamic topology. ACM
D. Hahn and C. Wojtan. 2016. Fast approximations for boundary element based brittle               Trans Graph 32, 3 (2013), 27.
    fracture simulation. ACM Trans Graph 35, 4 (2016), 104.                                   J. Nairn. 2003. Material point method calculations with explicit cracks. Computer
J. Hegemann, C. Jiang, C. Schroeder, and J. Teran. 2013. A level set method for ductile           Modeling in Engineering and Sciences 4, 6 (2003), 649–664.
    fracture. In Proc ACM SIGGRAPH/Eurograp Symp Comp Anim. 193–201.                          R. Narain, A. Golas, and M. Lin. 2010. Free-flowing granular materials with two-way
Y. Hu. 2018. Taichi: An Open-Source Computer Graphics Library. arXiv preprint                     solid coupling. ACM Trans Graph 29, 6 (2010), 173:1–173:10.
    arXiv:1804.09293 (2018).                                                                  J. O’Brien, A. Bargteil, and J. Hodgins. 2002. Graphical modeling and animation of
Y. Hu, Y. Fang, Z. Ge, Z. Qu, Y. Zhu, A. Pradhana, and C. Jiang. 2018. A moving least             ductile fracture. In Proc ACM SIGGRAPH 2002. 291–294.
    squares Material Point Method with displacement discontinuity and two-way rigid           J. O’Brien and J. Hodgins. 1999. Graphical modeling and animation of brittle fracture.
    body coupling: supplementary document. 37, 4 (2018), 146:1–146:14.                            In Proceedings of the 26th annual conference on Computer graphics and interactive
A. Huerta, T. Belytschko, S. Fernández-Méndez, and T. Rabczuk. 2004. Meshfree methods.            techniques. ACM Press/Addison-Wesley Publishing Co., 137–146.
    (2004).                                                                                   M. Pauly, R. Keiser, B. Adams, P. Dutré, M. Gross, and L. J Guibas. 2005. Meshless
Thomas J.R. Hughes. 2012. The finite element method: Linear static and dynamic finite             animation of fracturing solids. ACM Trans Graph 24, 3 (2005), 957–964.
    element analysis. Courier Corporation.                                                    T. Pfaff, R. Narain, J. de Joya, and J. O’Brien. 2014. Adaptive tearing and cracking of
G. Irving, J. Teran, and R. Fedkiw. 2004. Invertible finite elements for robust simulation        thin sheets. ACM Trans Garph 33, 4 (2014), 110.
    of large deformation. In Proc ACM SIGGRAPH/Eurograph Symp Comp Anim. 131–140.             D. Ram, T. Gast, C. Jiang, C. Schroeder, A. Stomakhin, J. Teran, and P. Kavehpour. 2015.
C. Jiang, T. Gast, and J. Teran. 2017a. Anisotropic elastoplasticity for cloth, knit and          A material point method for viscoelastic fluids, foams and sponges. In Proc ACM
    hair frictional contact. ACM Trans Graph 36, 4 (2017).                                        SIGGRAPH/Eurograph Symp Comp Anim. 157–163.
C. Jiang, C. Schroeder, A. Selle, J. Teran, and A. Stomakhin. 2015. The affine particle-in-   A. Robinson-Mosher, R. English, and R. Fedkiw. 2009. Accurate tangential velocities for
    cell method. ACM Trans Graph 34, 4 (2015), 51:1–51:10.                                        solid fluid coupling. In Proc ACM SIGGRAPH/Eurograph Symp Comp Anim. ACM,
C. Jiang, C. Schroeder, and J. Teran. 2017b. An angular momentum conserving affine-               227–236.
    particle-in-cell method. J. Comput. Phys. 338 (2017), 137–164.                            A. Robinson-Mosher, T. Shinar, J. Gretarsson, J. Su, and R. Fedkiw. 2008. Two-way
C. Jiang, C. Schroeder, J. Teran, A. Stomakhin, and A. Selle. 2016. The material point            coupling of fluids to rigid and deformable solids and shells. ACM Trans Graph 27, 3
    method for simulating continuum materials. In SIGGRAPH 2016 Course. 24:1–24:52.               (2008), 46:1–46:9.
Y. Kanamori, N. Cuong, and T. Nishita. 2011. Local optimization of distortions in             S. Sato, Y. Dobashi, K. Iwasaki, T. Yamamoto, and T. Nishita. 2014. Deformation of 2D
    wide-angle images using moving least-squares. In Proceedings of the 27th Spring               flow fields using stream functions. In SIGGRAPH Asia 2014 Technical Briefs. ACM, 4.
    Conference on Computer Graphics. ACM, 51–56.                                              S. Schaefer, T. McPhail, and J. Warren. 2006. Image deformation using moving least
P. Kaufmann, S. Martin, M. Botsch, and M. Gross. 2009. Flexible simulation of deformable          squares. In ACM Trans Graph, Vol. 25. ACM, 533–540.
    models using discontinuous Galerkin FEM. Graphical Models 71, 4 (2009), 153–167.          R. Setaluri, M. Aanjaneya, S. Bauer, and E. Sifakis. 2014. SPGrid: A sparse paged grid
G. Klár, T. Gast, A. Pradhana Tampubolon, C. Fu, C. Schroeder, C. Jiang, and J. Teran.            structure applied to adaptive smoke simulation. ACM Trans Graph 33, 6 (2014), 205.
    2016. Drucker-prager elastoplasticity for sand animation. ACM Trans Graph 35, 4           T. Shinar, C. Schroeder, and R. Fedkiw. 2008. Two-way coupling of rigid and deformable
    (2016), 103:1–103:12.                                                                         bodies. In Proceedings of the 2008 ACM SIGGRAPH/Eurographics Symposium on
B. Klingner, B. Feldman, N. Chentanez, and J. O’Brien. 2006. Fluid animation with                 Computer Animation. Eurographics Association, 95–103.
    dynamic meshes. In ACM Trans Graph, Vol. 25. ACM, 820–825.                                E. Sifakis, K. Der, and R. Fedkiw. 2007. Arbitrary cutting of deformable tetrahedral-
D. Koschier and J. Bender. 2017. Density maps for improved SPH boundary handling.                 ized objects. In Proc ACM SIGGRAPH/Eurograph Symp Comp Anim. Eurographics
    In Proc ACM SIGGRAPH/Eurograph Symp Comp Anim. ACM, 1.                                        Association, 73–80.
D. Koschier, J. Bender, and N. Thuerey. 2017. Robust eXtended finite elements for             M. Steffen, R. Kirby, and M. Berzins. 2008. Analysis and reduction of quadrature errors
    complex cutting of deformables. ACM Trans Graph 36, 4 (2017), 55.                             in the material point method (MPM). Int J Numer Meth Eng 76, 6 (2008), 922–948.
P. Lancaster and K. Salkauskas. 1981. Surfaces generated by moving least squares              A. Stomakhin, C. Schroeder, L. Chai, J. Teran, and A. Selle. 2013. A material point
    methods. Mathematics of computation 37, 155 (1981), 141–158.                                  method for snow simulation. ACM Trans Graph 32, 4 (2013), 102:1–102:10.
T. Langlois, S. An, K. Jin, and D. James. 2014. Eigenmode compression for modal sound         A. Stomakhin, C. Schroeder, C. Jiang, L. Chai, J. Teran, and A. Selle. 2014. Augmented
    models. ACM Trans Graph 33, 4 (2014), 40.                                                     MPM for phase-change and varied materials. ACM Trans Graph 33, 4 (2014), 138:1–
D. Levin. 1998. The approximation power of moving least-squares. Mathematics of                   138:11.
    Computation of the American Mathematical Society 67, 224 (1998), 1517–1531.               D. Sulsky, S. Zhou, and H. Schreyer. 1995. Application of a particle-in-cell method to
D. Levin. 2004. Mesh-independent surface interpolation. In Geometric modeling for                 solid mechanics. Comp Phys Comm 87, 1 (1995), 236–252.
    scientific visualization. Springer, 37–49.                                                A. Pradhana Tampubolon, T. Gast, G. Klár, C. Fu, J. Teran, C. Jiang, and K. Museth. 2017.
C. Li, T. Zhang, and D. Goldman. 2013. A terradynamics of legged locomotion on                    Multi-species simulation of porous sand and water mixtures. ACM Trans Graph 36,
    granular media. Science 339, 6126 (2013), 1408–1412.                                          4 (2017).
W. Liu, S. Jun, and Y. Zhang. 1995. Reproducing kernel particle methods. International        D. Terzopoulos and K. Fleischer. 1988. Modeling inelastic deformation: viscolelasticity,
    journal for numerical methods in fluids 20, 8-9 (1995), 1081–1106.                            plasticity, fracture. SIGGRAPH Comp Graph 22, 4 (1988), 269–278.
F. Losasso, T. Shinar, A. Selle, and R. Fedkiw. 2006. Multiple interacting liquids. In ACM    Y. Wang, C. Jiang, C. Schroeder, and J. Teran. 2014. An adaptive virtual node algorithm
    Trans Graph, Vol. 25. ACM, 812–819.                                                           with robust mesh cutting. In Proc ACM SIGGRAPH/Eurograph Symp Comp Anim.
M. Macklin and M. Müller. 2013. Position based fluids. ACM Trans Graph 32, 4 (2013),              Eurographics Association, 77–85.
    104:1–104:12.                                                                             C. Wojtan, N. Thürey, M. Gross, and G. Turk. 2009. Deforming meshes that split and
M. Macklin, M. Müller, N. Chentanez, and T. Kim. 2014. Unified particle physics for               merge. In ACM Trans Graph, Vol. 28. ACM, 76.
    real-time applications. ACM Trans Graph 33, 4 (2014), 153:1–153:12.                       J. Wretborn, R. Armiento, and K. Museth. 2017. Animation of crack propagation by
S. Martin, P. Kaufmann, M. Botsch, E. Grinspun, and M. Gross. 2010. Unified simulation            means of an extended multi-body solver for the material point method. Computers
    of elastic rods, shells, and solids. In ACM Transactions on Graphics (TOG), Vol. 29.          & Graphics (2017).
    ACM, 39.                                                                                  J. Wu, R. Westermann, and C. Dick. 2015. A survey of physically based simulation of
N. Mitchell, M. Aanjaneya, R. Setaluri, and E. Sifakis. 2015. Non-manifold level sets:            cuts in deformable bodies. In Comp Graph Forum, Vol. 34. Wiley Online Library,
    A multivalued implicit surface representation with applications to self-collision             161–187.
    processing. ACM Trans Graph 34, 6 (2015), 247.                                            H. Xu and J. Barbič. 2014. Signed distance fields for polygon soup meshes. In Proceedings
N. Molino, Z. Bao, and R. Fedkiw. 2005. A virtual node algorithm for changing mesh                of Graphics Interface 2014. Canadian Information Processing Society, 35–41.
    topology during simulation. In ACM SIGGRAPH 2005 Courses. ACM, 4.                         Y. Yue, B. Smith, C. Batty, C. Zheng, and E. Grinspun. 2015. Continuum foam: a
G. Moutsanidis, D. Kamensky, D.Z. Zhang, Y. Bazilevs, and C.C. Long. 2018. Modeling               material point method for shear-dependent flows. ACM Trans Graph 34, 5 (2015),
    sub-grid scale discontinuities in the Material Point Method using a single velocity           160:1–160:20.
    field. Submitted, received via private communication (2018).                              O. Zarifi and C. Batty. 2017. A positive-definite cut-cell method for strong two-way
M. Müller, D. Charypar, and M. Gross. 2003. Particle-based fluid simulation for interac-          coupling between fluids and deformable bodies. In Proc ACM SIGGRAPH/Eurograph
    tive applications. In Symp Comp Anim (SCA ’03). 154–159.                                      Symp Comp Anim. ACM, 7.
M. Müller and M. Gross. 2004. Interactive virtual materials. In Proceedings of Graphics       Y. Zhu and R. Bridson. 2005. Animating sand as a fluid. ACM Trans Graph 24, 3 (2005),
    Interface 2004 (GI ’04). Canadian Human-Computer Commu, 239–246.                              965–972.
M. Müller, B. Heidelberger, M. Hennix, and J. Ratcliff. 2007. Position based dynamics. J      Y. Zhu and S. Gortler. 2007. 3D deformation using moving least squares. (2007).
    Vis Comm Imag Repre 18, 2 (2007), 109–118.



ACM Transactions on Graphics, Vol. 37, No. 4, Article 146. Publication date: August 2018.

```
