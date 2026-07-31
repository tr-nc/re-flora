# DDGI reference set

This directory keeps the three DDGI references used for the environment-probe work in Re: Flora. The PDFs are unmodified copies downloaded from the publishers' official pages. The Markdown files are original, searchable reading notes with PDF page, section, and figure anchors; they are not full-text conversions.

## Papers

| Formal title | Complete author list | Year and venue | PDF | Searchable notes | Primary source |
| --- | --- | --- | --- | --- | --- |
| *Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields* | Zander Majercik; Jean-Philippe Guertin; Derek Nowrouzezahrai; Morgan McGuire | 2019, *Journal of Computer Graphics Techniques* 8(2), 1–30 | [PDF](majercik-2019-ddgi.pdf) | [Notes](majercik-2019-ddgi.md) | [JCGT article page](https://jcgt.org/published/0008/02/01/) |
| *Scaling Probe-Based Real-Time Dynamic Global Illumination for Production* | Zander Majercik; Adam Marrs; Josef Spjut; Morgan McGuire | 2021, *Journal of Computer Graphics Techniques* 10(2), 1–29 | [PDF](majercik-2021-scaling-ddgi.pdf) | [Notes](majercik-2021-scaling-ddgi.md) | [JCGT article page](https://jcgt.org/published/0010/02/01/) |
| *Improving Probes in Dynamic Diffuse Global Illumination* | Dominik Roháček | 2022, *Proceedings of CESCG 2022: The 26th Central European Seminar on Computer Graphics* (non-peer-reviewed) | [PDF](rohacek-2022-improving-probes-ddgi.pdf) | [Notes](rohacek-2022-improving-probes-ddgi.md) | [CESCG publication page](https://cescg.org/cescg_submission/improving-probes-in-dynamic-diffuse-global-illumination/) |

The CESCG page and the [Charles University repository record](https://dspace.cuni.cz/handle/20.500.11956/171782?locale-attribute=en) both give the author's formal spelling as **Dominik Roháček**. Tomáš Iser is identified as the supervisor, not a coauthor.

## Formal citations

- Zander Majercik, Jean-Philippe Guertin, Derek Nowrouzezahrai, and Morgan McGuire. “Dynamic Diffuse Global Illumination with Ray-Traced Irradiance Fields.” *Journal of Computer Graphics Techniques (JCGT)* 8, no. 2 (2019): 1–30. <https://jcgt.org/published/0008/02/01/>.
- Zander Majercik, Adam Marrs, Josef Spjut, and Morgan McGuire. “Scaling Probe-Based Real-Time Dynamic Global Illumination for Production.” *Journal of Computer Graphics Techniques (JCGT)* 10, no. 2 (2021): 1–29. <https://jcgt.org/published/0010/02/01/>.
- Dominik Roháček. “Improving Probes in Dynamic Diffuse Global Illumination.” *Proceedings of CESCG 2022: The 26th Central European Seminar on Computer Graphics* (non-peer-reviewed), 2022. <https://cescg.org/cescg_submission/improving-probes-in-dynamic-diffuse-global-illumination/>.

## License and conversion decision

The final page of each JCGT PDF states that its authors provide the work under [Creative Commons Attribution-NoDerivatives 3.0](https://creativecommons.org/licenses/by-nd/3.0/) (CC BY-ND 3.0). That license permits redistribution of the unmodified PDF with attribution, but its NoDerivatives condition makes a reformatted full-text Markdown edition inappropriate to publish here. The corresponding Markdown files are therefore independently written reading notes.

Neither the CESCG publication page nor its PDF states a reuse license. The official PDF is preserved unmodified for project reference, but permission to redistribute an adapted full-text version could not be established. Its Markdown companion is likewise an original reading note and uses no extended quotation. Confirm rights before redistributing the CESCG PDF outside this repository.

License/source review date: 2026-08-01.

## Related implementation references

- [Adding global illumination to my game engine w/ DDGI [Voxel Devlog #23]](https://www.youtube.com/watch?v=L1vhle74AEU), by Douglas — a voxel-engine DDGI integration walkthrough. Title and channel display name were verified through YouTube's oEmbed metadata on 2026-08-01.

## File provenance and integrity

| File | Official download URL | Pages | SHA-256 |
| --- | --- | ---: | --- |
| `majercik-2019-ddgi.pdf` | <https://jcgt.org/published/0008/02/01/paper-lowres.pdf> | 30 | `4571db6e48ae3f4feaf1fd99b06e45d515672d82daba98dc402ae24d6769ce90` |
| `majercik-2021-scaling-ddgi.pdf` | <https://jcgt.org/published/0010/02/01/paper-lowres.pdf> | 29 | `273397f6b9bdde59bf9bcca49e4185702e0356f99eb7efe396dafe19f4cf22b5` |
| `rohacek-2022-improving-probes-ddgi.pdf` | <https://cescg.org/wp-content/uploads/2022/04/Rohacek-Improving-Probes-in-Dynamic-Diffuse-Global-Illumination.pdf> | 7 | `cd68ba4bb7eab86a2ebe9364e230fd5c1855eda73cb23f8cb27876d194943905` |

The JCGT downloads are the publishers' official low-resolution editions; their text and pagination match the full-resolution editions while avoiding roughly 145 MiB of unnecessary repository growth.

## Preparation and validation

- Downloaded with `curl 8.15.0` following redirects from the primary-source URLs above.
- Inspected with Poppler `pdfinfo 25.07.0`; all three PDFs are unencrypted and report the expected page counts.
- Extracted temporary text with Poppler `pdftotext 25.07.0` to verify searchability, headings, citations, and license notices. Extracted text was not committed.
- Wrote the Markdown notes as a synthesis of the papers, retaining explicit PDF page and figure pointers for verification.
