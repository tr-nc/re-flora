# ElevenLabs-generated SFX: commercial-use check

Checked: **2026-10-23**. This is a source-based licensing summary, not legal advice or a substitute for a separate agreement with ElevenLabs.

## Conclusion for this project

The user reports using the **Free plan** for the recently imported ElevenLabs SFX. Under the current published policy, these outputs are **not cleared for commercial use**, including the planned commercial game. Attribution does not replace a commercial license. Upgrading later does not automatically license outputs created before the paid subscription. [1]

Affected bundles (originals and edited derivatives):

- `assets/sfx/sprinkler/`
- `assets/sfx/apple-dirt/`
- `assets/sfx/leaves-breeze/`
- `assets/sfx/leaves-strong-wind/`

Cropping, crossfading, trimming, renaming, mixing or converting the Free-plan output is not evidence of a new commercial license. Treat all these originals/derivatives as **not for commercial release** until replaced by appropriately licensed generations or covered by explicit written permission. No assets were deleted, reprocessed or published as part of this check.

## Primary-source findings

### 1. Free-plan output is not commercial-use licensed

ElevenLabs' official publication FAQ states:

> “The free plan does **not** include a commercial license and cannot be used for any commercial purpose.”

It further states:

> “Content created outside of a paid subscription (before or after) cannot be used commercially and always requires attribution when shared non-commercially.”

For noncommercial publication of these non-music outputs, the FAQ specifies including `elevenlabs.io` or `11.ai` in the title. That attribution condition is **not** permission for commercial publication. The FAQ also directs users to product-specific terms. [1]

### 2. Paid-plan generations can be used commercially, subject to conditions

The FAQ states that all paid plans include a commercial license, excluding Beta Services, subject to necessary intellectual-property rights, applicable law, the Terms of Service, Prohibited Use Policy and service-specific terms. It says content generated during a paid subscription can be used commercially indefinitely, subject to those terms. The pricing page lists a Commercial License under Starter. [1][2]

Consequences:

- The ordinary route is to subscribe to a qualifying paid plan **and generate replacement takes during that subscription**.
- Merely upgrading, downloading the old generation again, or editing the old file does not satisfy the stated requirement that the content was generated during a paid subscription. [1]
- The exact sound of a new generation is not guaranteed to match the Free-plan take. If an existing take must be retained, request explicit written commercial authorization from ElevenLabs for those specific files; availability and cost of such authorization are unknown.
- Confirm the model/feature used is not subject to Beta restrictions; the screenshots identify SFX v3 but are not a legal determination of its service status. [1]

### 3. Game incorporation is allowed where the plan permits; isolated-file distribution is restricted

The current Sound Effects terms URL redirects to **ElevenCreative Terms**, last updated **09 October 2026**. Exhibit 4, section 2 explicitly permits incorporating SFX into “games” and “applications,” but qualifies commercial use with:

> “for commercial purposes only where your plan permits commercial use.”

The same section limits use to output combined with other material and prohibits standalone exploitation, making outputs available as isolated audio files or samples, and including them in sound/sample/effects libraries or collections, subject to the stated Community Sharing and express written permission exceptions. [3]

This is separate from the Free-plan restriction: **a paid subscription is not a general license to publish the raw WAVs as a reusable asset pack or unrestricted public download**. A public repository exposing the WAVs raises this isolated-file-distribution issue. Do not push these raw assets publicly without establishing permission for that distribution method. Embedding legitimately licensed SFX into a game is explicitly contemplated, but this check did not determine requirements for every game package/source-distribution layout. [3]

### 4. “Royalty-free” / “free generator” marketing does not remove plan conditions

The publication FAQ and product-specific terms explicitly condition commercial use on the subscription/license. Free generation credits and descriptions such as royalty-free should not be read as a Free-plan commercial license. [1][3]

## Recommended next steps

1. Do not ship these Free-plan outputs in the commercial game or use them in commercial promotional releases; no blanket permission for commercial-project prototyping is asserted here. [1]
2. Choose either paid-plan regeneration, explicit written clearance for the existing files, or replacement assets with suitable commercial rights.
3. Preserve evidence for replacements/clearances: generation date, qualifying plan, invoice/account record, applicable terms, exact source and processing history. Store the non-sensitive provenance with each bundle; do not commit billing credentials or personal account details.
4. Resolve raw-asset/public-repository distribution separately from in-game use. No remote visibility was inspected and no push was performed during this check.
5. Only after clearance, update the per-bundle rights status and proceed with runtime integration/release validation.

## Follow-up: cheapest paid plan and a public GitHub repository

Rechecked the official pricing and Sound Effects product pages on 2026-10-23. Both list **Starter at USD 6/month** with commercial use; the current first-month promotion is USD 1, subject to checkout/offer changes. There is no need to choose Creator solely to obtain the advertised SFX commercial-use permission. Generation must still occur during the qualifying paid subscription and satisfy the other conditions above. [1][2][4]

A public **source-code repository** and a commercial game can coexist without publishing the restricted audio files:

- Keep public code, generation prompts/settings, asset manifests and openly redistributable fallback/procedural sounds in GitHub.
- Keep paid-generated originals and selected derivatives in a private asset repository or private storage, with provenance alongside them. Authorized builds can obtain those assets privately and incorporate them into the game distribution under the applicable game-use license.
- Do not expose a public raw-audio download URL or automatically ship the raw library as a separate asset pack. Access control must cover the actual files, not just an index.
- Git LFS, WAV→OGG conversion, ZIP packing, renaming, or embedding the bytes in a source file does not by itself establish permission for the same standalone distribution.
- A public repo containing directly downloadable raw WAVs presents an isolated-output distribution concern under Exhibit 4 section 2. The fetched official material does **not** specifically resolve every public game-source-repo case; obtain written confirmation for that exact distribution model instead of assuming either a GitHub exception or a blanket prohibition on all public game code. [3]
- If the project must publish all sound assets for unrestricted source builds/reuse, use original recordings/procedural audio with rights you control, assets with an explicit suitable redistribution license, or a provider that expressly grants those rights. A model being open-source or a service saying "commercial use" is not, on its own, proof of output redistribution permission.

Suggested support question:

> On Starter, may I commit Sound Effects outputs generated during my paid subscription as individually downloadable WAV/OGG files in my commercial game's public GitHub source repository? The files would not be sold as an asset pack, but visitors could download them directly. Does Exhibit 4 section 2 permit this, and if so what licensing/attribution or additional written permission is required?

The Free-plan audio already exists in local Git commits from this task. `.gitignore` alone will not remove it from those commits. Before publishing those commits, choose an asset-distribution policy and perform any needed Git-history cleanup with explicit user authorization. This research did not remove files, rewrite history, change repository visibility, or push anything.

## Sources and retrieval notes

[1] ElevenLabs official documentation, **Can I publish the content I generate on the platform?**
https://elevenlabs.io/docs/help-center/legal/can-i-publish-the-content-i-generate-on-the-platform

Full page fetched. The equivalent `help.elevenlabs.io` Zendesk URL returned HTTP 403 on direct retrieval; the official docs-hosted version supplied the quoted text.

[2] ElevenLabs official **Pricing**
https://elevenlabs.io/pricing

Fetched; Starter lists Commercial License. Prices/promotions are not necessary to this conclusion and may change.

[3] ElevenLabs **ElevenCreative Terms**, **Exhibit 4: Sound Effects**, especially section 2, plus sections 3–5 and Community Sharing qualifications
https://elevenlabs.io/creative-terms

Full page fetched; displays “Last updated: 09 October 2026.” The legacy entry https://elevenlabs.io/sound-effects-terms redirects to this terms document. Any separate executed agreement, generation-time terms/notice history, or support-issued exception may affect the applicable rights; none was supplied or verified.

[4] ElevenLabs **Sound Effects**, product-specific pricing table
https://elevenlabs.io/sound-effects

Fetched on 2026-10-23; distinguishes Free (personal use only) from Starter (commercial use). Pricing is a current advertised offer, not a guaranteed future price.
