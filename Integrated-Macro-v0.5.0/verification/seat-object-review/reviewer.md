# Independent seat-object design review

Read-only review of `seat_selection.rs`, `vision.rs`, the Engine's retained marks, and `CandidateGeometry`. No code, test, build, application, or user configuration was changed or executed. The cases below are source-level reasoning examples, not new measured reproductions.

## What the current implementation actually does

Vision already forms a matching-color mask and separates it into four-neighbor connected components. A nonmatching one-pixel gap separates nearby seats; the algorithm does not require artificially spacing them farther apart. Diagonal corner contact alone also remains separate. The component's original pixels are discarded after extraction; only its bounding rectangle and pixel count survive in `Candidate.geometry`.

The duplicate guard is not a fixed-distance radius. It blocks the exact same click point, a new click point contained in the prior rectangle (or the reverse containment), or rectangle intersection of at least 75% of the smaller rectangle. Point containment is checked before the intersection ratio. A separate same-row/gap heuristic ranks adjacent groups, with separate seats as fallback; a large gap itself does not exclude a candidate.

## Cases that distinguish rectangles from masks

| Case | Current behavior | What retaining the actual mask changes |
| --- | --- | --- |
| Nearby rectangular seats separated by background | Separate components and non-overlapping rectangles; both remain eligible | No necessary behavioral change; should be a preservation control |
| A C/U-shaped component with a separate 2×2 object in its empty interior | Two components, but the first rectangle can contain the second click and falsely exclude it | Their pixel masks have no intersection, allowing a genuinely separate object to remain separate |
| Thick ring surrounding an empty area containing a separate object | Same rectangle-containment ambiguity | Hole-preserving masks avoid treating empty interior as occupied; indiscriminate hole filling restores the error |
| Separate diagonal objects whose rectangles overlap | Rectangle overlap/point containment can falsely identify them | Disjoint masks distinguish the visible objects; changing to eight-neighbor connectivity would instead merge some corner-touching seats |
| Selection leaves an inset or splits the original color into fragments | Bounding rectangles often correctly retain the original identity | Each fragment can be associated with the original mask by overlap, without enlarging the exclusion into neighboring objects |
| Same-color seats touch along an edge | Four-neighbor flood fill produces one component | Contouring the same binary mask adds no missing boundary; additional shape evidence or another signal is required to split instances reliably |

Mask overlap alone also has a counterexample. An original green thick ring can turn gray after selection while a green 2×2 marker appears in its previously empty center. The original and new matching-color masks are disjoint even though the site's hit target might be one seat. Filling the ring interior handles that example but incorrectly merges the genuine separate interior-object case. Thus a *matching-color mask*, an *entire object bounded against the background*, and a *persistent site seat identity* are different things.

Remembering initially separate labels does not resolve this ambiguity. Text or a background-colored vertical stroke can split one real seat into two initial matching-color components, each large enough to pass 2×2 support. Permanently exempting those labels from duplicate comparison can count the same seat twice. The current rectangle guard can also miss this case when the two component rectangles are disjoint. Initial separation is useful evidence but not a guarantee of different seats; shared outer boundaries, the intervening background and local changes must be considered together. A pixel-identical scene can represent either two site hit targets or one patterned hit target, so a screenshot alone cannot settle every case.

## Recommended direction

1. Preserve each candidate's original component mask and component label using the existing extraction traversal. Use rectangles only to cheaply locate possible overlaps, then compare the actual mask rather than treating every pixel inside the rectangle as occupied. Keep initially separate labels as evidence, without making label difference an unconditional exemption from duplicate checking.
2. Retain that original evidence only after delivered input, with the existing retained-attempt, failed-retry, map/region and new-run lifecycles. Selected fragments can refer back to the original instance instead of becoming new counted attempts.
3. For selected styling that creates new pixels outside the old matching mask, inspect the local before/after object boundary and neighboring labels. A tracked object may contain several color components, and its identity must not simply equal the initial component label. Do not automatically dilate all masks, fill every hole, or merge all nearby components. Such changes would recreate the adjacency loss the user is concerned about.
4. Keep same-row adjacency ranking separate from duplicate suppression. The confirmed preference is to try adjacent candidates first and fill the rest with separate candidates; absence of a clear adjacent group should not stall scanning.

This is a narrower, evidence-based improvement over rectangle-only identity, not a promise of generic object recognition. Touching identical objects, ambiguous borders, same-color decorations, and changed layouts can still be visually indistinguishable. Reuse component pixels or compact row spans rather than adding a full capture or whole-image pass per click. Keep masks out of profile data and diagnostic JSON; measure runtime and memory before claiming there is no speed cost.

## Separate questions that this does not solve

The three decisions must remain separate: **duplicate exclusion** asks whether a candidate represents a previously clicked object; **adjacency priority** chooses which distinct candidates to try first and permits separated-seat fallback; **selection counting** estimates how many delivered attempts remain. Improving one does not establish the others.

- **2×2 support** is a local click-area/noise check. It is neither a separation distance nor an object identity rule. Thin outlines and one-pixel seats can still be rejected even with improved object masks; changing this filter is a separate tradeoff.
- **Selection counting** remains an estimate based on delivered clicks. Better object identity can prevent counting one visible seat twice but cannot prove the site accepted a click, retained earlier selections, or completed a reservation.

Before implementation, the useful acceptance cases are separate close seats, concave/ring interiors, diagonal contact, edge contact, a single seat split into multiple components before any click, selected inset/split/recolored styles, a real neighbor appearing after a click, failed-seat retry with another seat retained, and adjacency fallback. The review deliberately did not run or add these tests in this discussion-only scope.
