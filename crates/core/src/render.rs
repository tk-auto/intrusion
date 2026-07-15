//! Rendering as a pure function of state (§11.1, §12.1).
//!
//! The grid the game draws is a pure function of state, and it prints as text —
//! which is what lets the whole UI be asserted in a native test with no browser
//! (§11.1). This module holds the smallest version of that: a facility in, a
//! grid of glyphs out. No colour categories (§11.2), no fog (§11.5a), no glyph
//! priority yet — those land with the renderer-interface ticket that formalises
//! this seam into a trait. The web crate's canvas renderer and any future text
//! or tile renderer all consume this same glyph grid.

use crate::facility::Facility;

/// Render a facility to a grid of glyphs, one `String` per row, top to bottom
/// (§11.1). Each character is the terrain's glyph (§10.3); floor is a space.
pub fn ascii_grid(facility: &Facility) -> Vec<String> {
    (0..facility.height())
        .map(|y| {
            (0..facility.width())
                .map(|x| {
                    facility
                        .terrain_at(x, y)
                        .expect("in-bounds by construction")
                        .glyph()
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The payoff of "render is a pure function that prints as text": a fixed
    /// state renders to a fixed grid we can eyeball. A 6×4 walled box is a
    /// hollow rectangle of `#`.
    #[test]
    fn walled_box_renders_as_a_hollow_rectangle() {
        let grid = ascii_grid(&Facility::walled_box(6, 4));
        assert_eq!(
            grid,
            vec![
                "######".to_string(),
                "#    #".to_string(),
                "#    #".to_string(),
                "######".to_string(),
            ]
        );
    }

    #[test]
    fn grid_dimensions_match_the_facility() {
        let facility = Facility::walled_box(40, 30);
        let grid = ascii_grid(&facility);
        assert_eq!(grid.len(), 30);
        assert!(grid.iter().all(|row| row.chars().count() == 40));
    }
}
