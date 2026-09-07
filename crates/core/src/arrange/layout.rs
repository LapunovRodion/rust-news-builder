//! Which layout a photograph gets, and which photographs share one (T093, FR-019).
//!
//! The rule is shape, not content:
//!
//! - A **landscape** photograph fills the container width. That is what the width is for.
//! - A **portrait** photograph floats, so the text wraps beside it rather than being pushed a
//!   screen further down by a column of face.
//! - **Neighbouring portraits pair into a row**, because two narrow images side by side read as
//!   a deliberate pair, while two consecutive floats read as an accident.
//! - Successive floats **alternate sides**, so a page of portraits does not grow a ragged
//!   column down one edge.
//!
//! Where there are more groups than there are places to put them, adjacent groups merge into
//! rows until they fit — surplus photographs are never dropped (FR-019's edge case).

use crate::model::item::Layout;
use crate::model::photo::{Photo, PhotoId};

/// A photograph's shape, which is all the layout rules need to know about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// Wider than it is tall, or square.
    Wide,
    /// Taller than it is wide.
    Tall,
}

impl Shape {
    /// The shape a photograph publishes at — after EXIF orientation, the editor's turns, and
    /// any crop, because that is what the reader will see.
    #[must_use]
    pub fn of(photo: &Photo) -> Self {
        if photo.is_portrait() {
            Self::Tall
        } else {
            Self::Wide
        }
    }
}

/// One planned placement: the photographs in it and the shape they share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The photographs, in item order.
    pub photos: Vec<PhotoId>,
    /// The shape of the first photograph, which decides a lone group's layout.
    pub shape: Shape,
}

/// The largest number of photographs put side by side without being asked to.
///
/// Four across is already small on a phone; beyond that the automatic answer stops being a
/// helpful default and becomes something the editor has to undo.
pub const MAX_AUTOMATIC_ROW: usize = 4;

/// Groups photographs into placements, then merges until they fit `slots`.
///
/// `slots` is how many points in the text are available. Passing zero means "as few groups as
/// the shape rules produce"; the caller then places them wherever it can.
#[must_use]
pub fn plan(photos: &[(PhotoId, Shape)], slots: usize) -> Vec<Group> {
    if photos.is_empty() {
        return Vec::new();
    }

    // Pass one: pair up neighbouring portraits, leave everything else alone.
    let mut groups: Vec<Group> = Vec::new();
    let mut index = 0usize;
    while index < photos.len() {
        let (id, shape) = photos[index];
        if shape == Shape::Tall
            && let Some((next_id, Shape::Tall)) = photos.get(index + 1).copied()
        {
            groups.push(Group {
                photos: vec![id, next_id],
                shape,
            });
            index += 2;
            continue;
        }
        groups.push(Group {
            photos: vec![id],
            shape,
        });
        index += 1;
    }

    // Pass two: if the text cannot hold that many, merge neighbours until it can. Merging left
    // to right keeps the result a function of the input alone (constitution IV).
    if slots > 0 {
        while groups.len() > slots {
            let Some(at) = merge_candidate(&groups) else {
                break;
            };
            let absorbed = groups.remove(at + 1);
            groups[at].photos.extend(absorbed.photos);
        }
    }

    groups
}

/// The leftmost adjacent pair that may merge: same shape first, then any pair, and never past
/// [`MAX_AUTOMATIC_ROW`] unless nothing else is available.
fn merge_candidate(groups: &[Group]) -> Option<usize> {
    let fits = |a: &Group, b: &Group| a.photos.len() + b.photos.len() <= MAX_AUTOMATIC_ROW;

    (0..groups.len().saturating_sub(1))
        .find(|at| groups[*at].shape == groups[at + 1].shape && fits(&groups[*at], &groups[at + 1]))
        .or_else(|| {
            (0..groups.len().saturating_sub(1)).find(|at| fits(&groups[*at], &groups[at + 1]))
        })
        .or(if groups.len() > 1 { Some(0) } else { None })
}

/// The layout for a planned group.
///
/// `floats_so_far` is how many floated placements have already been emitted for this item; it
/// is what makes successive floats alternate sides.
#[must_use]
pub fn layout_for(group: &Group, floats_so_far: usize) -> Layout {
    if group.photos.len() > 1 {
        return Layout::Row;
    }
    match group.shape {
        Shape::Wide => Layout::FullWidth,
        Shape::Tall => {
            if floats_so_far % 2 == 0 {
                Layout::FloatRight
            } else {
                Layout::FloatLeft
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Group, Shape, layout_for, plan};
    use crate::model::item::Layout;
    use crate::model::photo::PhotoId;

    fn photos(shapes: &[Shape]) -> Vec<(PhotoId, Shape)> {
        shapes
            .iter()
            .enumerate()
            .map(|(index, shape)| (PhotoId(index as u64 + 1), *shape))
            .collect()
    }

    fn sizes(groups: &[Group]) -> Vec<usize> {
        groups.iter().map(|g| g.photos.len()).collect()
    }

    #[test]
    fn nothing_in_nothing_out() {
        assert!(plan(&[], 3).is_empty());
    }

    #[test]
    fn landscapes_stay_on_their_own() {
        let groups = plan(&photos(&[Shape::Wide, Shape::Wide, Shape::Wide]), 5);
        assert_eq!(sizes(&groups), vec![1, 1, 1]);
    }

    #[test]
    fn neighbouring_portraits_pair_up() {
        let groups = plan(&photos(&[Shape::Tall, Shape::Tall]), 5);
        assert_eq!(sizes(&groups), vec![2]);
    }

    #[test]
    fn a_lone_trailing_portrait_stays_alone() {
        let groups = plan(&photos(&[Shape::Tall, Shape::Tall, Shape::Tall]), 5);
        assert_eq!(sizes(&groups), vec![2, 1]);
    }

    #[test]
    fn a_portrait_between_landscapes_is_not_paired() {
        let groups = plan(&photos(&[Shape::Wide, Shape::Tall, Shape::Wide]), 5);
        assert_eq!(sizes(&groups), vec![1, 1, 1]);
    }

    #[test]
    fn a_surplus_merges_until_it_fits() {
        let groups = plan(&photos(&[Shape::Wide; 6]), 2);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups.iter().map(|g| g.photos.len()).sum::<usize>(), 6);
    }

    #[test]
    fn merging_never_loses_a_photograph() {
        for count in 1..12usize {
            for slots in 1..5usize {
                let shapes: Vec<Shape> = (0..count)
                    .map(|n| if n % 3 == 0 { Shape::Tall } else { Shape::Wide })
                    .collect();
                let groups = plan(&photos(&shapes), slots);
                let total: usize = groups.iter().map(|g| g.photos.len()).sum();
                assert_eq!(total, count, "{count} photos into {slots} slots");
            }
        }
    }

    #[test]
    fn a_group_of_several_is_a_row() {
        let group = Group {
            photos: vec![PhotoId(1), PhotoId(2)],
            shape: Shape::Tall,
        };
        assert_eq!(layout_for(&group, 0), Layout::Row);
    }

    #[test]
    fn a_lone_landscape_fills_the_width() {
        let group = Group {
            photos: vec![PhotoId(1)],
            shape: Shape::Wide,
        };
        assert_eq!(layout_for(&group, 0), Layout::FullWidth);
        assert_eq!(layout_for(&group, 1), Layout::FullWidth);
    }

    #[test]
    fn successive_floats_alternate() {
        let group = Group {
            photos: vec![PhotoId(1)],
            shape: Shape::Tall,
        };
        assert_eq!(layout_for(&group, 0), Layout::FloatRight);
        assert_eq!(layout_for(&group, 1), Layout::FloatLeft);
        assert_eq!(layout_for(&group, 2), Layout::FloatRight);
    }
}
