//! Synthetic pixels, production vision and OwnedFrame sampling only.
//! No NativeAdapter, OS input, browser or real selected-seat status is supplied.
#[cfg(test)]
mod tests {
    use macro_core::{Candidate, Color, Point, VisionConfig};
    use macro_core::vision::{extract_candidates_view, color_matches, RgbaView};
    use macro_platform::OwnedFrame;

    const COLOR: Color = Color { r: 185, g: 224, b: 180 };
    const TOLERANCE: u8 = 12;
    const ACTIVE: [u8; 4] = [185, 224, 180, 255];
    const SELECTED: [u8; 4] = [50, 50, 50, 255];
    const EMPTY: [u8; 4] = [255, 255, 255, 255];

    fn frame() -> OwnedFrame {
        OwnedFrame { width: 200, height: 1150, stride: 800, rgba: EMPTY.repeat(200 * 1150),
            origin: macro_platform::Point { x: 0.0, y: 33.0 }, scale_x: 2.0, scale_y: 2.0,
            sequence: 1, captured_at_ns: 1, last_observed_ns: 1 }
    }
    fn fill(image: &mut OwnedFrame, x1: usize, y1: usize, x2: usize, y2: usize, color: [u8; 4]) {
        for y in y1..y2 { for x in x1..x2 {
            image.rgba[y * image.stride + x * 4..y * image.stride + x * 4 + 4].copy_from_slice(&color);
        }}
    }
    fn candidates(image: &OwnedFrame) -> Vec<Candidate> {
        extract_candidates_view(RgbaView { width: image.width as u32, height: image.height as u32,
            stride: image.stride, rgba: &image.rgba }, COLOR, TOLERANCE, &VisionConfig::default(), 0).unwrap()
    }
    fn pre_click_color_passes(image: &OwnedFrame, candidate: &Candidate) -> bool {
        // Use production OwnedFrame sampling and color matching. Runtime's
        // image_candidate_local calls exactly pixel_to_local for these integer
        // vision coordinates on macOS. Full Worker/native capture is not run.
        let local = image.pixel_to_local(candidate.point.x as usize, candidate.point.y as usize).unwrap();
        image.color_at(local).is_some_and(|[r, g, b]| color_matches(Color { r, g, b }, COLOR, TOLERANCE))
    }

    #[test]
    fn selected_style_with_outer_matching_pixels_keeps_the_id_control() {
        let mut image = frame(); fill(&mut image, 97, 911, 116, 933, ACTIVE);
        let original = candidates(&image).remove(0);
        fill(&mut image, 99, 913, 114, 931, SELECTED);
        let after = candidates(&image);
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, original.id);
        assert!(pre_click_color_passes(&image, &after[0]));
        println!("CONTROL unchanged outer extrema preserve {} even when the center is selected", original.id);
    }

    #[test]
    fn matching_inset_is_same_physical_seat_but_has_new_component_id() {
        let mut image = frame(); fill(&mut image, 97, 911, 116, 933, ACTIVE);
        let original = candidates(&image).remove(0);
        fill(&mut image, 97, 911, 116, 933, SELECTED);
        fill(&mut image, 100, 914, 113, 930, ACTIVE);
        let after = candidates(&image);
        assert_eq!(after.len(), 1);
        assert_ne!(after[0].id, original.id);
        assert_eq!(after[0].id, "pixel:100:914");
        assert!(pre_click_color_passes(&image, &after[0]));
        println!("REPRO same seat retained color inset: {} -> {}, {:?}, pre-click color=true", original.id, after[0].id, after[0].point);
    }

    #[test]
    fn selected_stroke_splits_one_seat_and_introduces_a_second_allowed_id() {
        let mut image = frame(); fill(&mut image, 97, 911, 116, 933, ACTIVE);
        let original = candidates(&image).remove(0);
        fill(&mut image, 106, 911, 108, 933, SELECTED);
        let after = candidates(&image);
        assert_eq!(after.len(), 2);
        assert!(after.iter().any(|c| c.id == original.id));
        let new = after.iter().find(|c| c.id != original.id).unwrap();
        assert_eq!(new.id, "pixel:108:911");
        assert!(pre_click_color_passes(&image, new));
        println!("REPRO one seat split by selected mark: retained {}, new {}, {:?}, pre-click color=true", original.id, new.id, new.point);
    }

    #[test]
    fn three_pixel_residue_matches_the_logged_second_candidate_and_passes_color() {
        let mut image = frame();
        // Fixture oracle: the nearby gray card is unavailable, but its residual
        // stroke retains the configured RGB. This status is not input to vision.
        fill(&mut image, 120, 911, 139, 933, [170, 170, 170, 255]);
        fill(&mut image, 122, 919, 123, 922, ACTIVE);
        fill(&mut image, 133, 1091, 152, 1113, ACTIVE); // full-size available seat control
        let after = candidates(&image);
        assert_eq!(after.len(), 2);
        let residue = after.iter().find(|c| c.id == "pixel:122:919").unwrap();
        assert_eq!(residue.point, Point { x: 122.0, y: 920.0 });
        assert!(pre_click_color_passes(&image, residue));
        assert!(pre_click_color_passes(&image, after.iter().find(|c| c.id == "pixel:133:1091").unwrap()));
        println!("REPRO synthetic unavailable card has 1x3 matching residue: {} {:?}; matches logged coordinates but does not prove the real frame had this shape", residue.id, residue.point);
        fill(&mut image, 122, 921, 123, 922, [170, 170, 170, 255]);
        assert_eq!(candidates(&image).len(), 1); // two pixels are rejected by existing minimum=3
        fill(&mut image, 122, 919, 123, 922, [170, 170, 170, 255]);
        assert!(!pre_click_color_passes(&image, residue)); // genuine changed RGB blocks before input
    }

    #[test]
    fn filter_tradeoff_controls_small_neighbors_and_a_hollow_seat() {
        let mut image = frame();
        fill(&mut image, 97, 911, 99, 913, ACTIVE);
        fill(&mut image, 100, 911, 102, 913, ACTIVE);
        let small = candidates(&image);
        assert_eq!(small.len(), 2);
        assert!(small.iter().all(|c| pre_click_color_passes(&image, c)));
        assert_eq!((small[1].point.x - small[0].point.x).abs(), 3.0);
        println!("TRADEOFF two genuine 2x2 fixture seats with a one-pixel gap remain separate: fixed radius >=3 would merge/exclude a valid neighbor");

        let mut hollow = frame();
        fill(&mut hollow, 97, 911, 116, 933, ACTIVE);
        fill(&mut hollow, 98, 912, 115, 932, EMPTY);
        let ring = candidates(&hollow);
        assert_eq!(ring.len(), 1);
        assert!(pre_click_color_passes(&hollow, &ring[0]));
        // Fixture arithmetic for a proposed filter, not production logic.
        let matching = |x: usize, y: usize| {
            let n = y * hollow.stride + x * 4;
            color_matches(Color { r: hollow.rgba[n], g: hollow.rgba[n+1], b: hollow.rgba[n+2] }, COLOR, TOLERANCE)
        };
        let solid_2x2 = (911..932).any(|y| (97..115).any(|x|
            matching(x,y) && matching(x+1,y) && matching(x,y+1) && matching(x+1,y+1)));
        assert!(!solid_2x2);
        println!("TRADEOFF valid hypothetical 19x22 hollow seat has 78 matching pixels and no solid 2x2; requiring solid support would reject this design");
    }
}
