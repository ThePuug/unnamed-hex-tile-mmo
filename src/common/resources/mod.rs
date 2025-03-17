pub mod map;

use std::collections::VecDeque;

use bevy::prelude::*;
use kiddo::{
    fixed::kdtree::{KdTree, Axis}, 
    traits::DistanceMetric
};
use fixed::{
    FixedI16,
    types::extra::U0, 
};

use crate::common::message::Event;

#[derive(Debug, Default, Resource)]
pub struct InputQueue(pub VecDeque<Event>);

#[derive(Resource)]
pub struct NNTree(pub KdTree<FixedI16<U0>, u64, 4, 8, u32>);

pub struct Hexhattan {}

impl<A: Axis, const K: usize> DistanceMetric<A, K> for Hexhattan {
    #[inline]
    fn dist(a: &[A; K], b: &[A; K]) -> A {
        let mut iter = a.iter()
            .zip(b.iter())
            .map(|(&a_val, &b_val)| {
                if a_val > b_val { a_val - b_val } 
                else { b_val - a_val }
            });
        let max = iter.by_ref().take(3).fold(A::ZERO, |a, b| a.max(b));
        iter.fold(max, |a, b| a + b)
    }

    #[inline]
    fn dist1(a: A, b: A) -> A {
        let diff: A = a.dist(b);
        diff * diff
    }
}
