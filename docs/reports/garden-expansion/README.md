# Garden expansion — offline report

Open **[index.html](index.html)** directly in a browser. Copy the **entire folder**, not only the HTML. No web server, internet connection, JavaScript runtime, font download or CDN is needed to read it. The HTML contains no scripts. External source links are optional and only visited when clicked.

- `index.html`: Chinese player-facing outcomes, all 14 implemented feature groups, 25 native screenshots, measured costs, test results and remaining acceptance limits.
- `assets/`: unchanged original PNG review captures, per-file provenance/dimensions/checksums in `manifest.json`, and the self-contained `README.md` provenance note.
- `evidence/validation.md`: aggregate acceptance, including all nine known baseline failures.
- `evidence/native-excerpts.txt`: explicitly selected stdout/stderr evidence, not complete raw logs. `log-sources.json` identifies the original transcripts by hash.
- `evidence/performance.json`: final all-frame flower/bee samples and reproducible median/nearest-rank-p95 statistics. Original paired grass records are in the native excerpts.
- `evidence/reproduce.md`: tested commands and interaction map; `bee-research.md`: retrieved primary source and access limitations.
- `evidence/browser-check.json`: desktop/mobile/copy/keyboard checks, zero findings for selected WCAG 2A/AA rules, and explicitly discarded/limited probes. HTTP/HTTPS were blocked; the CLI offline toggle did not change navigator state, so a hardware-disconnection test is not claimed.
- `verify.mjs`: optional offline integrity/link/statistics check. Run `node verify.mjs` from any directory; no packages are installed.

Validated runtime code: `6c5fe738`. Later report/evidence changes do not change the game. The full main test result was **1759 passed / 9 known baseline failures / 14 ignored**. Separate library target: 4 passed. Explicit integration targets: 1 passed / 4 ignored. Native checks are not manual mouse-play or listening approval; performance is not dense-scene certification. No public release, new push or rights clearance is claimed.

The screenshot collection is roughly 29 MiB. It intentionally preserves originals instead of silently cropping/recoloring captures. Images were captured across feature implementation stages, not all from one final game build. No audio or external photographs are bundled. See `assets/README.md` for rights/provenance qualifications.
