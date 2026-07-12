use {
    crate::{
        AnyLeaf, AnyNode, AnySlot, AnyTerm, Dual, DualError, ErasedBranch, ErasedLeaf, ErasedNode,
        ErasedSlot, Frontier, Registry, RootedHole, RootedLeaf, RootedPath, any_leaf, any_slot,
        check_dual, typed_node,
    },
    ahash::{HashMap, HashSet, HashSetExt as _},
    alloc::sync::Arc,
    core::{
        any::{Any, TypeId},
        iter,
        marker::PhantomData,
    },
    pbt::Pbt,
};

/// ADT: 1 * 1
#[derive(Clone, Debug, Eq, Hash, PartialEq, Pbt)]
pub struct Pair<Lhs, Rhs> {
    lhs: Lhs,
    rhs: Rhs,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum PairBranch {
    Pair = 0,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum PairLeaf {}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum PairNode {
    Pair = 0,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum PairSlot {
    Lhs = 0,
    Rhs = 1,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum PairPairSlot {
    Lhs = 0,
    Rhs = 1,
}

impl<Lhs: Dual, Rhs: Dual> Dual for Pair<Lhs, Rhs> {
    type Branch = PairBranch;
    type Leaf = PairLeaf;
    type Node = PairNode;
    type Slot = PairSlot;

    #[inline]
    fn fields(&self) -> Result<HashMap<Self::Slot, AnyTerm>, <Self as Dual>::Leaf> {
        let Self { ref lhs, ref rhs } = *self;
        Ok([
            (PairSlot::Lhs, AnyTerm::new::<Lhs>(lhs)),
            (PairSlot::Rhs, AnyTerm::new::<Rhs>(rhs)),
        ]
        .into_iter()
        .collect())
    }

    #[inline]
    fn fields_of_node(node: Self::Node) -> Result<HashSet<Self::Slot>, Self::Leaf> {
        match node {
            PairNode::Pair => Ok([PairSlot::Lhs, PairSlot::Rhs].into_iter().collect()),
        }
    }

    #[inline]
    fn from_node<F>(node: Self::Node, fields: F) -> Result<Self, DualError>
    where
        F: crate::Fields<Self>,
    {
        Ok(match node {
            PairNode::Pair => Self {
                lhs: fields.field::<Lhs>(PairSlot::Lhs)?,
                rhs: fields.field::<Rhs>(PairSlot::Rhs)?,
            },
        })
    }

    #[inline]
    fn register_all_field_types(registry: &mut Registry) {
        let () = registry.register::<Lhs>();
        let () = registry.register::<Rhs>();
    }

    #[inline]
    fn slot_type(slot: Self::Slot) -> TypeId {
        match slot {
            PairSlot::Lhs => TypeId::of::<Lhs>(),
            PairSlot::Rhs => TypeId::of::<Rhs>(),
        }
    }
}

impl From<PairBranch> for PairNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PairBranch) -> Self {
        match value {
            PairBranch::Pair => Self::Pair,
        }
    }
}

impl From<PairLeaf> for PairNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PairLeaf) -> Self {
        match value {}
    }
}

impl From<PairSlot> for PairBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PairSlot) -> Self {
        match value {
            PairSlot::Lhs | PairSlot::Rhs => Self::Pair,
        }
    }
}

impl From<PairBranch> for ErasedBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PairBranch) -> Self {
        Self(value as usize)
    }
}

impl From<PairLeaf> for ErasedLeaf {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PairLeaf) -> Self {
        Self(value as usize)
    }
}

impl From<PairNode> for ErasedNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PairNode) -> Self {
        Self(value as usize)
    }
}

impl From<PairSlot> for ErasedSlot {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PairSlot) -> Self {
        Self(value as usize)
    }
}

impl TryFrom<ErasedBranch> for PairBranch {
    type Error = ErasedBranch;

    #[inline]
    fn try_from(value: ErasedBranch) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::Pair,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedLeaf> for PairLeaf {
    type Error = ErasedLeaf;

    #[inline]
    fn try_from(value: ErasedLeaf) -> Result<Self, Self::Error> {
        Err(value)
    }
}

impl TryFrom<ErasedNode> for PairNode {
    type Error = ErasedNode;

    #[inline]
    fn try_from(value: ErasedNode) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::Pair,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedSlot> for PairSlot {
    type Error = ErasedSlot;

    #[inline]
    fn try_from(value: ErasedSlot) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::Lhs,
            1 => Self::Rhs,
            _ => return Err(value),
        })
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{DualError, binary_tree::BinaryTree, peano::Peano},
        ahash::HashMapExt as _,
        pbt::pbt,
    };

    check_dual!(Pair<Peano, BinaryTree>);
}
