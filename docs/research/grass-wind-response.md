# Grass response to sustained wind: evidence and animation guidance

Research date: 2026-09-16. Scope: crop/grass stem motion, not species-calibrated prediction of an individual grass blade.

## Evidence from three primary studies

1. **Py, de Langre & Moulia (2006), Journal of Fluid Mechanics 568, 425–449.** Field video measurements of wheat and alfalfa show coherent waving near the plants' free-vibration frequency, approximately independent of mean wind velocity over the measured conditions; the spatial wavelength increases with velocity. Their coupled linear model proposes frequency lock-in. The model retains one fundamental plant mode and explicitly simplifies its height dependence to `χ(y) = y/h`. That linear shape is a canopy-model approximation, not evidence that a real blade rotates as a rigid straight segment. [Paper and DOI](https://doi.org/10.1017/S0022112006002667), [author-hosted full text, especially §§2–3](https://yakari.polytechnique.fr/Django-pub/documents/py2006rp-1pp.pdf).

2. **Dupont et al. (2010), Journal of Fluid Mechanics 652, 5–44.** A large-eddy airflow simulation coupled to a single-mode crop oscillator reproduces measured alfalfa waving statistics. Instantaneous turbulent velocity supplies drag forcing; mean-field-only models omit the fluctuations important for movement. Coherent plant waving patches differ from the wind's coherent eddies. Crucially, this study finds negligible feedback of plant motion on wind under its tested conditions and no frequency lock-in, contrary to the earlier linear stability model. Therefore plant-frequency selection is useful evidence, but universal two-way lock-in is not established. [Paper and DOI](https://doi.org/10.1017/S0022112010000686), [author-hosted full text, abstract and §§2, 4–5](https://yakari.polytechnique.fr/Django-pub/documents/duponts2010rp-1pp.pdf).

3. **Speck & Spatz (2004), American Journal of Botany 91, 789–796.** Release experiments on giant reed, a grass-family plant, conducted when wind was not noticeable, show damped harmonic bending. Video tracks markers at different heights. Leaves increase damping, with structural energy transfer contributing in addition to aerodynamic resistance. This supports decaying inertial motion after excitation and distributed stem bending; it does not establish perpetual motion under constant, perfectly laminar wind. A several-metre reed is not a numerical calibration for lawn grass. [Full original article, methods and results](https://bsapubs.onlinelibrary.wiley.com/doi/full/10.3732/ajb.91.6.789).

## What follows, and what does not

For a stable passive oscillator, `m q'' + c q' + k q = F0` with positive damping and constant force approaches `q = F0/k`; its transient dies away. This is a mathematical consequence of that model, not a claim that actual outdoor wind is constant. A steady *mean* wind permits turbulence and other unsteady aerodynamic forcing. Self-excited flutter is a separate instability with geometry and flow conditions; none of the evidence above licenses “all grass always oscillates in laminar wind.”

Thus ongoing motion with a constant game wind vector needs an explicit interpretation: unresolved, wind-powered fluctuations or an artistic response model. Merely feeding a constant force into a damped spring cannot supply ongoing oscillation.

## Suggested bounded continuous animation model (design proposal)

These formulas are implementation choices, not fits or equations claimed by the papers.

- Continue sampling the authoritative local wind `U(x,t)`. Set a smooth magnitude envelope `e = |U|²/(|U|² + Uref²)`, and fade the directional component continuously near zero wind.
- Separate mean lean from vibration. Use bounded mean lean plus an oscillatory component about that lean. For a stateful implementation, drive the existing damped plant oscillator with `Fmean(U) + σ e ξ(x,t)`, where `ξ` is bounded, continuous, zero-mean, spatially correlated fine forcing. Describe it as unresolved wind excitation. Avoid independent large gusts that compete with the wind field.
- For an inexpensive procedural response, use `q = qmean + Amax e [0.75 sin(θ1) + 0.25 sin(θ2)]`. This has a known amplitude bound. Use stable per-plant phase/frequency variation and only weak spatial modulation; keep frequency mainly tied to plant type/size. This is an artistic surrogate, not a solved aeroelastic oscillator.
- If frequency changes, integrate phase (`θ += 2π f dt`) or retain constant seeded frequencies; `sin(time * f(currentWind))` jumps phase when wind changes. Smooth changing envelopes and directions too.
- Root anchoring is mandatory. One convenient curved shape is `φ(s)=s²(3−s)/2`, `s∈[0,1]`: displacement and slope vanish at the base, with unit tip displacement. This is the normalized small-deflection cantilever shape under a tip force, used here only as an illustrative animation approximation. It is not the exact grass eigenmode or the linear shape in the canopy papers. Do not apply the same translation to every vertex.
- Validate calm decay, constant nonzero wind, wind reversal, strength steps, sustained extreme strength, and frame-rate changes. Check finite bounded displacement and a stationary root separately from subjective appearance. No universal realistic frequency or damping value is established by these three papers for the game's grass.
