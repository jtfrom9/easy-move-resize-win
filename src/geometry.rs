//! Pure window-geometry logic for move/resize gestures.
//!
//! This module is intentionally free of any Win32 dependency so the core
//! behaviour can be unit-tested in isolation.

/// A window rectangle expressed as top-left origin plus size, in physical
/// screen pixels (the same coordinate space used by Win32 `SetWindowPos`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    /// Build a rect from Win32 left/top/right/bottom edges (as returned by
    /// `GetWindowRect`).
    pub fn from_ltrb(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Rect {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        }
    }
}

/// Which edges of the window follow the cursor during a resize gesture.
///
/// A horizontal pair (`left`/`right`) and a vertical pair (`top`/`bottom`) are
/// mutually exclusive: at most one of each can be set.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ResizeEdges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

/// Minimum window size enforced while resizing, so a window can never be
/// collapsed to nothing (or inverted) by an aggressive drag.
pub const MIN_SIZE: i32 = 80;

/// Decide which edges to drag for a resize, based on which 3x3 region of the
/// window the cursor grabbed.
///
/// * Outer columns/rows drag the corresponding edge (or corner when both axes
///   are outer).
/// * A middle band on one axis means that axis is not resized, letting the user
///   resize a single edge.
/// * The dead-centre region maps to the bottom-right corner (right + bottom
///   edges follow the cursor), so a centre grab behaves exactly like grabbing
///   that corner: dragging outward enlarges, dragging inward shrinks.
pub fn resize_edges_for_point(win: Rect, px: i32, py: i32) -> ResizeEdges {
    let col = band(px - win.x, win.w);
    let row = band(py - win.y, win.h);

    let mut e = ResizeEdges::default();
    match col {
        0 => e.left = true,
        2 => e.right = true,
        _ => {}
    }
    match row {
        0 => e.top = true,
        2 => e.bottom = true,
        _ => {}
    }

    // Dead centre: no edge selected on either axis -> bottom-right corner.
    if !e.left && !e.right && !e.top && !e.bottom {
        e.right = true;
        e.bottom = true;
    }
    e
}

/// Band classifier returning 0 (first third), 1 (middle third) or 2 (last
/// third) for `pos` within `size`.
fn band(pos: i32, size: i32) -> i32 {
    if size <= 0 {
        return 1;
    }
    let p = pos.clamp(0, size - 1);
    if p < size / 3 {
        0
    } else if p < 2 * size / 3 {
        1
    } else {
        2
    }
}

/// New rectangle after a move gesture: the window is translated by the cursor
/// delta, keeping its size.
pub fn moved(win: Rect, dx: i32, dy: i32) -> Rect {
    Rect {
        x: win.x + dx,
        y: win.y + dy,
        w: win.w,
        h: win.h,
    }
}

/// New rectangle after a resize gesture. The edges marked in `edges` follow the
/// cursor delta; the opposite edges stay anchored. Size is clamped to
/// [`MIN_SIZE`] without moving the anchored edge.
pub fn resized(win: Rect, edges: ResizeEdges, dx: i32, dy: i32) -> Rect {
    let mut r = win;

    if edges.left {
        // Right edge stays fixed; left edge moves.
        let right = win.x + win.w;
        let new_x = (win.x + dx).min(right - MIN_SIZE);
        r.x = new_x;
        r.w = right - new_x;
    } else if edges.right {
        r.w = (win.w + dx).max(MIN_SIZE);
    }

    if edges.top {
        // Bottom edge stays fixed; top edge moves.
        let bottom = win.y + win.h;
        let new_y = (win.y + dy).min(bottom - MIN_SIZE);
        r.y = new_y;
        r.h = bottom - new_y;
    } else if edges.bottom {
        r.h = (win.h + dy).max(MIN_SIZE);
    }

    r
}

/// New window rectangle for an in-progress drag, given the cursor delta from the
/// drag's start point. `resize` selects resize vs move semantics. This is the
/// pure core of the mouse-hook's per-move update.
pub fn drag_result(orig: Rect, edges: ResizeEdges, resize: bool, dx: i32, dy: i32) -> Rect {
    if resize {
        resized(orig, edges, dx, dy)
    } else {
        moved(orig, dx, dy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: Rect = Rect {
        x: 100,
        y: 100,
        w: 300,
        h: 300,
    };

    // ----- region classification -----

    #[test]
    fn top_left_region_drags_top_left_corner() {
        let e = resize_edges_for_point(WIN, 110, 110);
        assert_eq!(
            e,
            ResizeEdges {
                left: true,
                top: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn bottom_right_region_drags_bottom_right_corner() {
        let e = resize_edges_for_point(WIN, 390, 390);
        assert_eq!(
            e,
            ResizeEdges {
                right: true,
                bottom: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn middle_left_region_drags_left_edge_only() {
        // Left third horizontally, middle third vertically.
        let e = resize_edges_for_point(WIN, 110, 250);
        assert_eq!(
            e,
            ResizeEdges {
                left: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn top_middle_region_drags_top_edge_only() {
        let e = resize_edges_for_point(WIN, 250, 110);
        assert_eq!(
            e,
            ResizeEdges {
                top: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn dead_center_defaults_to_bottom_right() {
        let e = resize_edges_for_point(WIN, 250, 250);
        assert_eq!(
            e,
            ResizeEdges {
                right: true,
                bottom: true,
                ..Default::default()
            }
        );
    }

    // ----- move -----

    #[test]
    fn move_translates_without_changing_size() {
        let r = moved(WIN, 50, -30);
        assert_eq!(
            r,
            Rect {
                x: 150,
                y: 70,
                w: 300,
                h: 300
            }
        );
    }

    // ----- resize -----

    #[test]
    fn resize_right_bottom_grows_size_only() {
        let e = ResizeEdges {
            right: true,
            bottom: true,
            ..Default::default()
        };
        let r = resized(WIN, e, 40, 20);
        assert_eq!(
            r,
            Rect {
                x: 100,
                y: 100,
                w: 340,
                h: 320
            }
        );
    }

    #[test]
    fn resize_left_top_keeps_opposite_corner_anchored() {
        let e = ResizeEdges {
            left: true,
            top: true,
            ..Default::default()
        };
        // Drag the top-left corner 50px right and 50px down: origin moves,
        // size shrinks, and the bottom-right corner (400,400) stays put.
        let r = resized(WIN, e, 50, 50);
        assert_eq!(
            r,
            Rect {
                x: 150,
                y: 150,
                w: 250,
                h: 250
            }
        );
        assert_eq!(r.x + r.w, WIN.x + WIN.w);
        assert_eq!(r.y + r.h, WIN.y + WIN.h);
    }

    #[test]
    fn resize_respects_minimum_size_when_growing_inward() {
        let e = ResizeEdges {
            right: true,
            bottom: true,
            ..Default::default()
        };
        // Shrink far past the minimum.
        let r = resized(WIN, e, -1000, -1000);
        assert_eq!(r.w, MIN_SIZE);
        assert_eq!(r.h, MIN_SIZE);
    }

    #[test]
    fn resize_left_clamps_without_crossing_anchor() {
        let e = ResizeEdges {
            left: true,
            ..Default::default()
        };
        // Drag left edge far to the right; it must stop MIN_SIZE from the
        // fixed right edge and never invert.
        let r = resized(WIN, e, 1000, 0);
        assert_eq!(r.w, MIN_SIZE);
        assert_eq!(r.x + r.w, WIN.x + WIN.w);
    }

    // ----- region classification: remaining corners/edges -----

    #[test]
    fn top_right_region_drags_top_right_corner() {
        let e = resize_edges_for_point(WIN, 390, 110);
        assert_eq!(
            e,
            ResizeEdges {
                right: true,
                top: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn bottom_left_region_drags_bottom_left_corner() {
        let e = resize_edges_for_point(WIN, 110, 390);
        assert_eq!(
            e,
            ResizeEdges {
                left: true,
                bottom: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn middle_right_region_drags_right_edge_only() {
        let e = resize_edges_for_point(WIN, 390, 250);
        assert_eq!(
            e,
            ResizeEdges {
                right: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn bottom_middle_region_drags_bottom_edge_only() {
        let e = resize_edges_for_point(WIN, 250, 390);
        assert_eq!(
            e,
            ResizeEdges {
                bottom: true,
                ..Default::default()
            }
        );
    }

    // ----- band boundaries -----

    #[test]
    fn exact_first_third_boundary_is_middle_band() {
        // px - x == w/3 (== 100): `p < size/3` is false, so this is the middle
        // band (no horizontal edge from the column).
        let e = resize_edges_for_point(WIN, 200, 250);
        assert!(!e.left && !e.right || (e.right && e.bottom));
        // Middle column + middle row -> dead centre -> bottom-right.
        assert_eq!(
            e,
            ResizeEdges {
                right: true,
                bottom: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn exact_second_third_boundary_is_last_band() {
        // px - x == 2*w/3 (== 200) -> last band (right column).
        let e = resize_edges_for_point(WIN, 300, 250);
        assert_eq!(
            e,
            ResizeEdges {
                right: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn size_not_divisible_by_three_has_no_off_by_one() {
        let win = Rect {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        // third == 33, two-thirds == 66. Offset 33 is the middle column...
        let mid = resize_edges_for_point(win, 33, 50);
        assert!(!mid.left); // not the left column
                            // ...offset 66 is the right column.
        let right = resize_edges_for_point(win, 66, 50);
        assert!(right.right);
    }

    #[test]
    fn zero_sized_window_falls_back_to_bottom_right() {
        let win = Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
        let e = resize_edges_for_point(win, 5, 5);
        assert_eq!(
            e,
            ResizeEdges {
                right: true,
                bottom: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn negative_sized_window_falls_back_to_bottom_right() {
        let win = Rect {
            x: 0,
            y: 0,
            w: -10,
            h: -10,
        };
        let e = resize_edges_for_point(win, 5, 5);
        assert_eq!(
            e,
            ResizeEdges {
                right: true,
                bottom: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn cursor_outside_window_is_clamped_to_nearest_band() {
        // Left of the window -> left column; right of the window -> right column.
        let left = resize_edges_for_point(WIN, 50, 250);
        assert_eq!(
            left,
            ResizeEdges {
                left: true,
                ..Default::default()
            }
        );
        let right = resize_edges_for_point(WIN, 999, 250);
        assert_eq!(
            right,
            ResizeEdges {
                right: true,
                ..Default::default()
            }
        );
    }

    // ----- single-edge resize -----

    #[test]
    fn resize_right_only_ignores_dy() {
        let e = ResizeEdges {
            right: true,
            ..Default::default()
        };
        let r = resized(WIN, e, 40, 99);
        assert_eq!(
            r,
            Rect {
                x: 100,
                y: 100,
                w: 340,
                h: 300
            }
        );
    }

    #[test]
    fn resize_bottom_only_ignores_dx() {
        let e = ResizeEdges {
            bottom: true,
            ..Default::default()
        };
        let r = resized(WIN, e, 99, 20);
        assert_eq!(
            r,
            Rect {
                x: 100,
                y: 100,
                w: 300,
                h: 320
            }
        );
    }

    #[test]
    fn resize_top_edge_clamps_to_min_size_with_bottom_anchored() {
        let e = ResizeEdges {
            top: true,
            ..Default::default()
        };
        let r = resized(WIN, e, 0, 1000);
        assert_eq!(r.h, MIN_SIZE);
        assert_eq!(r.y + r.h, WIN.y + WIN.h);
    }

    #[test]
    fn resize_moderate_shrink_stays_above_min_size() {
        let e = ResizeEdges {
            right: true,
            bottom: true,
            ..Default::default()
        };
        let r = resized(WIN, e, -50, -50);
        assert_eq!(
            r,
            Rect {
                x: 100,
                y: 100,
                w: 250,
                h: 250
            }
        );
    }

    // ----- move edge cases -----

    #[test]
    fn move_zero_delta_is_identity() {
        assert_eq!(moved(WIN, 0, 0), WIN);
    }

    #[test]
    fn move_negative_delta_is_unbounded() {
        let r = moved(WIN, -500, -500);
        assert_eq!(
            r,
            Rect {
                x: -400,
                y: -400,
                w: 300,
                h: 300
            }
        );
    }

    // ----- pure drag dispatch / conversion helpers -----

    #[test]
    fn drag_result_dispatches_move_and_resize() {
        let move_r = drag_result(WIN, ResizeEdges::default(), false, 50, -30);
        assert_eq!(move_r, moved(WIN, 50, -30));

        let edges = ResizeEdges {
            right: true,
            bottom: true,
            ..Default::default()
        };
        let resize_r = drag_result(WIN, edges, true, 40, 20);
        assert_eq!(resize_r, resized(WIN, edges, 40, 20));
    }

    #[test]
    fn rect_from_ltrb_computes_size() {
        assert_eq!(Rect::from_ltrb(100, 100, 400, 400), WIN);
    }

    #[test]
    fn resize_with_no_edges_is_identity() {
        let r = resized(WIN, ResizeEdges::default(), 999, 999);
        assert_eq!(r, WIN);
    }

    #[test]
    fn resize_left_edge_grows_with_negative_dx() {
        let e = ResizeEdges {
            left: true,
            ..Default::default()
        };
        // Drag the left edge further left: origin x decreases, width grows, and
        // the right edge stays anchored.
        let r = resized(WIN, e, -50, 0);
        assert_eq!(
            r,
            Rect {
                x: 50,
                y: 100,
                w: 350,
                h: 300
            }
        );
        assert_eq!(r.x + r.w, WIN.x + WIN.w);
    }

    #[test]
    fn resize_top_edge_grows_with_negative_dy() {
        let e = ResizeEdges {
            top: true,
            ..Default::default()
        };
        let r = resized(WIN, e, 0, -50);
        assert_eq!(
            r,
            Rect {
                x: 100,
                y: 50,
                w: 300,
                h: 350
            }
        );
        assert_eq!(r.y + r.h, WIN.y + WIN.h);
    }

    #[test]
    fn one_pixel_window_classifies_deterministically() {
        // size==1 -> size/3 == 0, so the only valid offset (0) lands in the last
        // band on both axes: bottom-right corner, not the dead-centre fallback.
        let win = Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        };
        let e = resize_edges_for_point(win, 0, 0);
        assert_eq!(
            e,
            ResizeEdges {
                right: true,
                bottom: true,
                ..Default::default()
            }
        );
    }

    #[test]
    fn rect_from_ltrb_does_not_normalise() {
        assert_eq!(Rect::from_ltrb(100, 100, 100, 100), Rect { x: 100, y: 100, w: 0, h: 0 });
        assert_eq!(
            Rect::from_ltrb(200, 200, 100, 100),
            Rect {
                x: 200,
                y: 200,
                w: -100,
                h: -100
            }
        );
    }
}
