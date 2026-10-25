# Sun / runtime moon sticker

- `Sun.png`: pre-existing original 16×16 RGBA pixel-art sun used by the game.
  Creator, storefront URL and original download filename are unavailable in this
  checkout; the bundle name is `Planets_16x16`. No attribution is invented.
- `License.txt`: original bundled license. It allows modifications in larger
  derivative works for the license holder, but forbids redistribution as standalone
  stock assets. Purchase/license-holder evidence is not present here; this note
  does not establish new redistribution rights.
- There is **no separate generated moon image**. The renderer reuses the existing
  sun alpha/silhouette and texel detail and remaps its palette at runtime, so the
  original file is unmodified. No external service, AI image model or new asset
  download is involved.

Original SHA-256:
`24f5b283e7215a754ecf1f1e98fb1a5da8d09dd70405599280b83024afb2ba67`.

Repeatable derivative definition: `composition_sky.slang` samples `Sun.png` with
its existing sRGB texture decode and alpha. Compute
`sqrt(clamp(dot(rgb, (0.2126, 0.7152, 0.0722)), 0, 1))`; interpolate in linear RGB
from slate `(0.19, 0.25, 0.39)` to ivory blue `(0.82, 0.88, 1.0)`. Retain alpha.
Moon display size is 85% of the current sun size, direction is opposite the live
sun, and visibility fades with the existing night factor and above-horizon gate.
It is composed after stars, including the shared glass/reflection sky path.
This is an initial full-moon sticker, not a lunar phase/orbit or moonlight model.
