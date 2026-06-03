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

/// ADT: 1 + 1
#[derive(Clone, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum Boolean {
    False,
    True,
}

// #[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum BooleanBranch {
    // n/a
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum BooleanLeaf {
    False = 0,
    True = 1,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum BooleanNode {
    False = 0,
    True = 1,
}

// #[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum BooleanSlot {
    // n/a
}

impl Dual for Boolean {
    type Branch = BooleanBranch;
    type Leaf = BooleanLeaf;
    type Node = BooleanNode;
    type Slot = BooleanSlot;

    #[inline]
    fn fields(&self) -> Result<HashMap<Self::Slot, AnyTerm<'_>>, <Self as Dual>::Leaf> {
        match *self {
            Self::False => Err(BooleanLeaf::False),
            Self::True => Err(BooleanLeaf::True),
        }
    }

    #[inline]
    fn fields_of_node(node: Self::Node) -> Result<HashSet<Self::Slot>, Self::Leaf> {
        match node {
            BooleanNode::False => Err(BooleanLeaf::False),
            BooleanNode::True => Err(BooleanLeaf::True),
        }
    }

    #[inline]
    fn from_node<F>(node: Self::Node, fields: F) -> Result<Self, DualError>
    where
        F: crate::Fields<Self>,
    {
        Ok(match node {
            BooleanNode::False => Self::False,
            BooleanNode::True => Self::True,
        })
    }

    #[inline]
    fn register_all_field_types(_registry: &mut Registry) {
        // you *could* put `Self` here (and, in macros, we should for full generality);
        // it'll just do nothing, since `register` short-circuits on already-registered types.
    }

    #[inline]
    fn slot_type(slot: Self::Slot) -> TypeId {
        match slot {}
    }
}

impl From<BooleanBranch> for BooleanNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BooleanBranch) -> Self {
        match value {}
    }
}

impl From<BooleanLeaf> for BooleanNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BooleanLeaf) -> Self {
        match value {
            BooleanLeaf::False => BooleanNode::False,
            BooleanLeaf::True => BooleanNode::True,
        }
    }
}

impl From<BooleanSlot> for BooleanBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BooleanSlot) -> Self {
        match value {}
    }
}

impl From<BooleanBranch> for ErasedBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BooleanBranch) -> Self {
        Self(value as usize)
    }
}

impl From<BooleanLeaf> for ErasedLeaf {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BooleanLeaf) -> Self {
        Self(value as usize)
    }
}

impl From<BooleanNode> for ErasedNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BooleanNode) -> Self {
        Self(value as usize)
    }
}

impl From<BooleanSlot> for ErasedSlot {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BooleanSlot) -> Self {
        Self(value as usize)
    }
}

impl TryFrom<ErasedBranch> for BooleanBranch {
    type Error = ErasedBranch;

    #[inline]
    fn try_from(value: ErasedBranch) -> Result<Self, Self::Error> {
        Err(value)
    }
}

impl TryFrom<ErasedLeaf> for BooleanLeaf {
    type Error = ErasedLeaf;

    #[inline]
    fn try_from(value: ErasedLeaf) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::False,
            1 => Self::True,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedNode> for BooleanNode {
    type Error = ErasedNode;

    #[inline]
    fn try_from(value: ErasedNode) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::False,
            1 => Self::True,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedSlot> for BooleanSlot {
    type Error = ErasedSlot;

    #[inline]
    fn try_from(value: ErasedSlot) -> Result<Self, Self::Error> {
        Err(value)
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::DualError, pbt::pbt};

    check_dual!(Boolean);
}
