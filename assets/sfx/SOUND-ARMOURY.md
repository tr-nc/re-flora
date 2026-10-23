# Sound Armoury — SFX Mega Bundle provenance

## Purchase and source

- Creator/publisher: **Sound Armoury**; the storefront also credits **Matt J Hart**.
- Product: **SFX Mega Bundle** — https://sound-armoury.itch.io/sfx-mega-bundle
- Purchase date shown on the owner's itch.io confirmation: **2025-07-09**.
- Evidence: the owner supplied the public product URL and a purchase-confirmation
  screenshot. The screenshot, account/email, price and private download URL are
  not stored in this repository. This note records the review, not a transferable
  receipt or a grant of ownership to repository users.

The public bundle listing includes **Rocks & Bricks**, **Buttons**, and
**SynFauna: Crickets & Cicadas**. The owner explicitly confirmed that the terrain loop, UI button and both cicada
calls share this bundle purchase as their source. They are grouped together in
the asset catalog; this does not merge their playback roles or change WAV paths. The first two WAVs' `encoded_by` tags identify
Sound Armoury, and their library suffixes match the listed products. The cicada
package name, original filenames and conversion hashes were recorded earlier.
This audit did not download the purchased archives or compare the first two WAVs
against a pristine archive; exact original archive versions remain unverified.
The current storefront is not an archived list of the bundle's 2025 contents.

## Installed audio

| File / folder | Library | Runtime use | Installed format |
| --- | --- | --- | --- |
| `ROCKMisc_Designed Rock Movement Loop A_SARM_RkBrck_Stereo-Loop.wav` | Rocks & Bricks | Terrain-edit movement loop | 96 kHz, stereo, PCM24, 25.578375 s |
| `MECHSwtch_Game Boy Advance SP, B Button, On 05_SARM_BTNS.wav` | Buttons | Inventory/UI feedback | 96 kHz, mono, PCM24, 0.094073 s |
| `summer_cicadas/` (two WAVs) | SynFauna: Crickets & Cicadas | Finite spatial calls at ecology sites | 48 kHz, mono, PCM16; [processing and original hashes](summer_cicadas/provenance.json) |

The root WAVs' historical processing is unknown; do not infer that a 48 kHz
recommendation in the SFX guide describes their actual encoding. No samples,
metadata, gain, duration or encoding were changed in this provenance update.

SHA-256 of the installed root WAVs:

```text
1e2d52ba047701c1e4449953b4b72265ab5880f94a4d3b7cf870e5d6805f97b4  ROCKMisc_Designed Rock Movement Loop A_SARM_RkBrck_Stereo-Loop.wav
56a0c6263647929a1c9235df46e2f06e2f2e98fd3df26019da1ebbef1365ac1d  MECHSwtch_Game Boy Advance SP, B Button, On 05_SARM_BTNS.wav
```

## Applicable public terms

Reviewed 2026-10-23. Both the product page and creator profile link the
**Sound Armoury End User Licence Agreement** specifically covering itch.io,
not the different store-wide licenses used by Unity Asset Store / A Sound Effect:

- Creator profile: https://sound-armoury.itch.io/
- Public EULA: https://drive.google.com/file/d/1KS3b6fp_ulzvbuZRgXyl_SFrKeL2ctJz/view?usp=sharing
- [Complete extracted EULA text](SOUND-ARMOURY-LICENSE.txt).

The EULA permits commercial and non-commercial media projects (including games),
modified or unmodified audio, and project-team sharing under its stated conditions.
Attribution is appreciated but not required. It prohibits generative-AI training,
political campaigning without agreement, and redistribution of the content for
free or a price without explicit permission, including modified content.

**Decision:** purchased game use is supported by the current public terms;
standalone/raw WAV distribution in a public repository is **not cleared** and
requires explicit permission or a private/replacement asset plan. This is not
an open asset license, and GPL/CC labels elsewhere do not relicense these WAVs.
Do not confuse permitted incorporation into a game with publishing reusable sound
files. Team sharing does not give teammates rights to unrelated projects.

The one-page source PDF has SHA-256
`dbe4c3981179b7bd00f15cd76ad0d32355f0106b23e9c5437333ade90afdfd26`.
The text was extracted with pypdf (line-end whitespace/outer blank lines removed);
the PDF itself is not committed. The public PDF has no stated effective/version
date; this is a current terms snapshot, not proof of the exact purchase-time terms.

Neither the sounds nor Git history were removed by this documentation change.
Runtime startup still requires all four files; arrange fallback/replacements
before removing active assets. No special redistribution permission was supplied.
