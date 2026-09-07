//! Where photographs go in the text (T092, FR-019).
//!
//! Pure arithmetic over counts: how many paragraphs there are and how many groups of photos
//! have to be fitted between them. Nothing here knows what a photograph looks like, which is
//! what keeps the rule instant, deterministic (constitution IV), and readable as a table of
//! small cases in the tests below.
//!
//! Two rules shape the answer:
//!
//! - **Never before the first paragraph.** An item that opens with a picture and no words
//!   reads as an untitled photograph. Where there is text to lead with, the arrangement leads
//!   with it.
//! - **Spread, don't pile.** Groups are laid out at even fractions of the way through the
//!   text, so three photographs in six paragraphs land after the second, the fourth and the
//!   sixth rather than all after the first.

/// How many paragraphs precede each group, one entry per group, strictly increasing where the
/// text is long enough to allow it.
///
/// `paragraphs` is the number of paragraphs in the body; `groups` the number of placements to
/// fit. With no paragraphs at all every group lands at the front, in order — there is nowhere
/// else for them to be.
#[must_use]
pub fn boundaries(paragraphs: usize, groups: usize) -> Vec<usize> {
    if groups == 0 {
        return Vec::new();
    }
    if paragraphs == 0 {
        return vec![0; groups];
    }

    let mut out = Vec::with_capacity(groups);
    let mut previous = 0usize;
    for index in 0..groups {
        // Even fractions of the way through: group i of k sits at (i+1)/(k+1) of the text.
        let ideal = ((index + 1) * paragraphs).div_ceil(groups + 1);
        // At least one paragraph of lead, and at least one paragraph past the group before —
        // until the text runs out, at which point the tail stacks at the end.
        let at = ideal.max(1).max(previous + 1).min(paragraphs);
        out.push(at);
        previous = at;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::boundaries;

    #[test]
    fn nothing_to_place_needs_no_boundaries() {
        assert!(boundaries(5, 0).is_empty());
    }

    #[test]
    fn with_no_text_everything_lands_at_the_front() {
        assert_eq!(boundaries(0, 3), vec![0, 0, 0]);
    }

    #[test]
    fn one_group_in_a_long_text_sits_around_the_middle() {
        let at = boundaries(6, 1);
        assert_eq!(at.len(), 1);
        assert!(at[0] >= 2 && at[0] <= 4, "{at:?}");
    }

    #[test]
    fn groups_are_spread_and_never_share_a_boundary() {
        let at = boundaries(6, 3);
        assert_eq!(at.len(), 3);
        for pair in at.windows(2) {
            assert!(pair[1] > pair[0], "{at:?}");
        }
    }

    #[test]
    fn nothing_is_placed_before_the_first_paragraph_when_there_is_one() {
        for groups in 1..8 {
            let at = boundaries(4, groups);
            assert!(at.iter().all(|n| *n >= 1), "{groups}: {at:?}");
        }
    }

    #[test]
    fn more_groups_than_paragraphs_stack_at_the_end_rather_than_overflowing() {
        let at = boundaries(2, 5);
        assert_eq!(at.len(), 5);
        assert!(at.iter().all(|n| *n <= 2), "{at:?}");
        assert_eq!(at.last(), Some(&2));
    }

    #[test]
    fn the_answer_is_monotonic() {
        // A later group is never placed earlier than an earlier one, whatever the counts.
        for paragraphs in 0..10 {
            for groups in 0..10 {
                let at = boundaries(paragraphs, groups);
                for pair in at.windows(2) {
                    assert!(pair[1] >= pair[0], "{paragraphs}/{groups}: {at:?}");
                }
            }
        }
    }
}
