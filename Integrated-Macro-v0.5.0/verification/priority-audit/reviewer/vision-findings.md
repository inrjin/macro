# P3 / P5 measured findings

Command:

```sh
CARGO_HOME="$PWD/.build-tools/cargo" RUSTUP_HOME="$PWD/.build-tools/rustup" RUSTUP_TOOLCHAIN=stable PATH="$PWD/.build-tools/cargo/bin:$PATH" cargo test --offline -p macro-core --test priority_vision_audit -- --nocapture
```

The new integration test directly calls production extraction and, where indicated, the real Engine. Input images are synthetic RGBA. Raw output is `priority-vision-results.txt`. Five passing tests mean their assertions and observed limitations reproduced; they do not mean every scenario succeeds.

## P3: transient seats

The declared sampling interval is 20 ms. Twenty integer phase offsets were enumerated for each seat lifetime; this is not a measured operating-system frame rate or a statistical success rate.

| Visible lifetime | Phases with an observed seat and proposed click | Phases missed by sampling |
| --- | ---: | ---: |
| 1 ms | 1 / 20 | 19 / 20 |
| 4 ms | 4 / 20 | 16 / 20 |
| 8 ms | 8 / 20 | 12 / 20 |
| 16 ms | 16 / 20 | 4 / 20 |
| 40 ms | 20 / 20 | 0 / 20 |

All observed seat components produced a proposal in these otherwise-empty maps. A seat that appears and disappears between supplied images cannot be recovered by the extraction algorithm. No native posting, pre-click revalidation, capture backlog, site processing, or physical latency was measured here.

## P5: geometry is not seat identity

- A same-color, same-size legend inside the ROI is accepted. With that legend above one real seat, extraction returns two candidates and the Engine proposes the legend first. Cropping the legend out or using the existing configuration's exclusion rectangle removes it in this fixture. The test does not imply the UI offers a dedicated exclusion editor.
- Two 5×5 seat patches with a one-pixel gap produce two candidates. Edge-adjacent patches produce one connected component and one candidate. A 1- or 2-pixel seat is rejected by the default minimum of three matching pixels.
- The extractor ignores only alpha zero. Alpha 1, 64 and 249 still produce candidates; the platform's separate pre-click `color_at` guard rejects alpha below 250. This shows an extraction/validation difference, not proof that a transparent candidate receives an OS click.
- A 400-pixel nonseat exceeds a configured maximum of 25 pixels and is rejected. Cropping the ROI to 25 pixels inside that same nonseat makes the cropped component eligible. Component size is measured within the ROI, not across the original object.

Production changes were not made by this reviewer. Aligning extraction opacity with the platform guard is a small consistency candidate. Automatically classifying a legend, separating touching seats, or rejecting clipped objects needs a separate accuracy/speed decision: stricter geometry can also discard legitimate small or partially visible seats. ROI correctness remains necessary under the current algorithm.
