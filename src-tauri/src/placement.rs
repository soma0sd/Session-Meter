//! Physical-coordinate placement inside the union of monitor work areas.

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            w: w.into(),
            h: h.into(),
        }
    }

    fn right(self) -> i64 {
        self.x + self.w
    }
    fn bottom(self) -> i64 {
        self.y + self.h
    }
}

/// Count visible pixels once, even when work areas overlap. Each vertical strip has a
/// constant set of covering work areas; merge their clipped vertical intervals.
fn visible_area(window: Rect, areas: &[Rect]) -> i64 {
    let mut edges = vec![window.x, window.right()];
    for area in areas {
        edges.push(area.x.clamp(window.x, window.right()));
        edges.push(area.right().clamp(window.x, window.right()));
    }
    edges.sort_unstable();
    edges.dedup();
    let mut total = 0;
    for strip in edges.windows(2) {
        let mut intervals: Vec<_> = areas
            .iter()
            .filter_map(|area| {
                if area.x >= strip[1] || area.right() <= strip[0] {
                    return None;
                }
                let top = area.y.max(window.y);
                let bottom = area.bottom().min(window.bottom());
                (top < bottom).then_some((top, bottom))
            })
            .collect();
        intervals.sort_unstable();
        let mut end = window.y;
        let mut height = 0;
        for (top, bottom) in intervals {
            height += (bottom - top.max(end)).max(0);
            end = end.max(bottom);
        }
        total += (strip[1] - strip[0]) * height;
    }
    total
}

/// Keep a fully visible rectangle unchanged, including one spanning adjacent monitors.
/// Otherwise choose a fully visible position with the smallest squared displacement.
/// If the window cannot fit anywhere, maximize its visible area before minimizing distance.
pub fn closest_position(areas: &[Rect], x: i32, y: i32, w: i32, h: i32) -> (i32, i32, bool) {
    let areas: Vec<_> = areas
        .iter()
        .copied()
        .filter(|r| r.w > 0 && r.h > 0)
        .collect();
    if areas.is_empty() || w <= 0 || h <= 0 {
        return (x, y, false);
    }
    let window = Rect::new(x, y, w, h);
    let full_area = window.w * window.h;
    if visible_area(window, &areas) == full_area {
        return (x, y, false);
    }

    // Visibility can change only when a window edge crosses a work-area edge. Between
    // those coordinates full-visibility regions are axis-aligned rectangles, so their
    // nearest point has each coordinate at the requested position or one of these edges.
    let mut xs = vec![window.x];
    let mut ys = vec![window.y];
    for area in &areas {
        xs.extend([
            area.x,
            area.right(),
            area.x - window.w,
            area.right() - window.w,
        ]);
        ys.extend([
            area.y,
            area.bottom(),
            area.y - window.h,
            area.bottom() - window.h,
        ]);
    }
    for values in [&mut xs, &mut ys] {
        values.retain(|v| i32::try_from(*v).is_ok());
        values.sort_unstable();
        values.dedup();
    }
    let mut best = None;
    for &cx in &xs {
        for &cy in &ys {
            let candidate = Rect {
                x: cx,
                y: cy,
                ..window
            };
            let visible = visible_area(candidate, &areas);
            let dx = i128::from(cx - window.x);
            let dy = i128::from(cy - window.y);
            // Coordinates break equal-distance ties deterministically, independent of
            // monitor enumeration order. Wide arithmetic also handles remote saved positions.
            let score = (-visible, dx * dx + dy * dy, cx, cy);
            if best.is_none_or(|current| score < current) {
                best = Some(score);
            }
        }
    }
    let (_, _, nx, ny) = best.expect("requested position is always a candidate");
    (nx as i32, ny as i32, nx != window.x || ny != window.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side_by_side() -> [Rect; 2] {
        [Rect::new(0, 0, 1920, 1040), Rect::new(1920, 0, 1920, 1040)]
    }

    #[test]
    fn individual_and_group_widgets_can_cross_a_visible_seam() {
        let areas = side_by_side();
        for (x, w, h) in [(1630, 300, 200), (1140, 800, 300)] {
            assert_eq!(closest_position(&areas, x, 100, w, h), (x, 100, false));
        }
    }

    #[test]
    fn incremental_drag_reaches_the_other_monitor() {
        let areas = side_by_side();
        let mut x = 1600;
        for _ in 0..100 {
            x = closest_position(&areas, x + 10, 100, 300, 200).0;
        }
        assert_eq!(x, 2600);
    }

    #[test]
    fn vertically_adjacent_monitors_allow_spanning() {
        let areas = [Rect::new(0, -1080, 1920, 1080), Rect::new(0, 0, 1920, 1040)];
        assert_eq!(
            closest_position(&areas, 200, -100, 300, 200),
            (200, -100, false)
        );
    }

    #[test]
    fn negative_coordinates_and_different_physical_monitor_sizes_are_valid() {
        let areas = [
            Rect::new(-2560, -400, 2560, 1400),
            Rect::new(0, 0, 1920, 1040),
        ];
        assert_eq!(
            closest_position(&areas, -1500, -200, 450, 300),
            (-1500, -200, false)
        );
        assert_eq!(
            closest_position(&areas, -100, 100, 450, 300),
            (-100, 100, false)
        );
    }

    #[test]
    fn fully_offscreen_recovers_to_nearest_position_not_last_monitor() {
        let areas = side_by_side();
        assert_eq!(
            closest_position(&areas, -400, 100, 300, 200),
            (0, 100, true)
        );
        assert_eq!(
            closest_position(&areas, 4000, 100, 300, 200),
            (3540, 100, true)
        );
        let reversed = [areas[1], areas[0]];
        assert_eq!(
            closest_position(&reversed, -400, 100, 300, 200),
            (0, 100, true)
        );
    }

    #[test]
    fn gap_between_monitors_is_not_a_display_area() {
        let areas = [Rect::new(0, 0, 1000, 800), Rect::new(1200, 0, 1000, 800)];
        assert_eq!(
            closest_position(&areas, 900, 100, 300, 200),
            (700, 100, true)
        );
        assert_eq!(
            closest_position(&areas, 1100, 100, 300, 200),
            (1200, 100, true)
        );
    }

    #[test]
    fn staggered_monitor_corner_is_not_a_display_area() {
        let areas = [Rect::new(0, 0, 1000, 800), Rect::new(1000, 200, 1000, 800)];
        assert_eq!(
            closest_position(&areas, 900, 100, 300, 200),
            (900, 200, true)
        );
    }

    #[test]
    fn taskbar_and_other_reserved_areas_remain_excluded() {
        let areas = [Rect::new(0, 0, 1920, 1040), Rect::new(1920, 0, 1920, 1000)];
        assert_eq!(
            closest_position(&areas, 1800, 900, 300, 200),
            (1800, 800, true)
        );
        assert_eq!(
            closest_position(&areas[..1], 100, 1000, 300, 200),
            (100, 840, true)
        );
    }

    #[test]
    fn overlapping_work_areas_do_not_double_count_visibility() {
        let areas = [Rect::new(0, 0, 1000, 800), Rect::new(500, 0, 1000, 800)];
        assert_eq!(visible_area(Rect::new(400, 700, 700, 200), &areas), 70000);
        assert_eq!(
            closest_position(&areas, 400, 700, 700, 200),
            (400, 600, true)
        );
    }

    #[test]
    fn group_wider_than_one_monitor_can_fit_across_the_union() {
        assert_eq!(
            closest_position(&side_by_side(), 1500, 100, 2200, 300),
            (1500, 100, false)
        );
    }

    #[test]
    fn oversized_window_maximizes_visibility_and_then_minimizes_movement() {
        let areas = [Rect::new(0, 0, 1000, 800)];
        assert_eq!(
            closest_position(&areas, 100, 100, 1200, 200),
            (0, 100, true)
        );
        assert_eq!(
            closest_position(&areas, -100, 100, 1200, 200),
            (-100, 100, false)
        );
    }

    #[test]
    fn removed_monitor_and_resized_window_recover_to_nearest_edge() {
        let areas = [Rect::new(0, 0, 1920, 1040)];
        assert_eq!(
            closest_position(&areas, 2200, 300, 300, 200),
            (1620, 300, true)
        );
        assert_eq!(
            closest_position(&areas, 1620, 800, 800, 300),
            (1120, 740, true)
        );
    }

    #[test]
    fn invalid_or_missing_geometry_leaves_position_unchanged() {
        assert_eq!(closest_position(&[], 123, 456, 300, 200), (123, 456, false));
        assert_eq!(
            closest_position(&[Rect::new(0, 0, 0, 800)], 123, 456, 300, 200),
            (123, 456, false)
        );
        assert_eq!(
            closest_position(&side_by_side(), 123, 456, 0, 200),
            (123, 456, false)
        );
    }

    #[test]
    fn extreme_saved_coordinates_do_not_overflow() {
        assert_eq!(
            closest_position(&side_by_side(), i32::MIN, i32::MAX, 300, 200),
            (0, 840, true)
        );
    }

    #[test]
    fn nearest_position_matches_exhaustive_search_for_small_layouts() {
        let layouts = [
            vec![Rect::new(-4, -2, 5, 6), Rect::new(1, 0, 5, 4)],
            vec![Rect::new(-4, -2, 4, 4), Rect::new(2, 1, 4, 4)],
            vec![Rect::new(-4, -2, 6, 4), Rect::new(-1, 0, 5, 5)],
        ];
        for areas in layouts {
            for (w, h) in [(2, 2), (4, 3), (8, 2)] {
                for x in -6..8 {
                    for y in -4..7 {
                        let score = |cx: i32, cy: i32| {
                            let dx = i128::from(cx - x);
                            let dy = i128::from(cy - y);
                            (
                                -visible_area(Rect::new(cx, cy, w, h), &areas),
                                dx * dx + dy * dy,
                                cx,
                                cy,
                            )
                        };
                        let expected = (-12..=12)
                            .flat_map(|cx| (-12..=12).map(move |cy| (cx, cy)))
                            .map(|(cx, cy)| score(cx, cy))
                            .min()
                            .unwrap();
                        let (nx, ny, _) = closest_position(&areas, x, y, w, h);
                        assert_eq!(
                            score(nx, ny),
                            expected,
                            "position=({x},{y}), size=({w},{h}), areas={areas:?}"
                        );
                    }
                }
            }
        }
    }
}
