# Empty-state association: read-only design review

## Confirmed failure

The current reader searches `empty` over the whole frame, but searches a count digit only on the heading's immediate line. Therefore a wrapped count under a heading can be missed while an unrelated panel's empty-state label supplies zero. Independent fixture `unrelated_empty_with_wrapped_count_three` demonstrates this. The labels individually matching their templates does not establish their relationship.

## Required distinction

Explicit adjacent `선택 좌석 0` is local count evidence. A heading plus `선택된 좌석이 없습니다` requires evidence that both belong to the same seat-selection container. A missing digit alone is never zero. Unrecognized text beneath the heading must not be silently treated as an empty panel.

## Practical supported-layout proposal

1. Find the heading and explicit empty-state text without fixed screen coordinates.
2. Detect the same selection container from visible boundaries: sustained background/edge separation and frame edges where applicable. Use the `선택 완료` control as an additional semantic anchor, not as a substitute for container boundaries. Heading, empty label and completion control must share that container, in their supported vertical order. Boundary search is relative to detected content, not a hardcoded right-hand screen region.
3. Reject competing count text, seat-list content or another panel separator in the proposed empty-state region. If the container cannot be distinguished visually, return Unreadable. Multiple compatible containers or anchors are ambiguous.

This is a narrow NOL layout contract. It is not a generic OCR parser or a proof of HTML ancestry.

## Rejected shortcuts and adversarial cases

- Nearest empty label, horizontal overlap or an arbitrary heading-distance limit can still pair unrelated panels, especially after responsive wrapping.
- White-background flood fill can join the panel, the top toolbar and white areas of the seat map. It does not identify a container.
- Heading/empty/button order alone fails when a full-width footer button spans two unrelated panels. Require local panel evidence.
- Absence of a same-line digit does not exclude a wrapped count or seat list. Preserve the demonstrated negative fixture.
- A border/button requirement changes the supported positive contract. Existing synthetic bare heading+empty examples must become Unreadable instead of being retained as positive by weakening the new association test. Add complete container fixtures and actual zero-state screenshot validation.

## Validation before adoption

Use a real zero-state screenshot to establish visible boundaries, then independently render translated/scaled/cropped complete panels, neighboring unrelated empty panels, wrapped nonzero counts, shared-footer layouts, multiple panels and missing boundaries. Positive supported scenes must read zero; unsupported/contradictory scenes must remain Unreadable. The latest real count-three image shows a separate light selection panel and completion control, but it does not by itself validate the empty-state layout. No production changes or tests were executed for this design review.
