use {
    crate::{
        AnyLeaf, AnyNode, AnySlot, AnyTerm, Dual, DualError, ErasedBranch, ErasedLeaf, ErasedNode,
        ErasedSlot, Frontier, Registry, RootedHole, RootedLeaf, RootedPath, any_leaf, any_slot,
        check_dual_roundtrip, typed_node,
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
pub enum NthBoolean {
    Successor(Box<Self>),
    False,
    True,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum NthBooleanBranch {
    Successor = 0,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum NthBooleanLeaf {
    False = 1,
    True = 2,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum NthBooleanNode {
    Successor = 0,
    False = 1,
    True = 2,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum NthBooleanSlot {
    Successor0 = 0,
}

impl Dual for NthBoolean {
    type Branch = NthBooleanBranch;
    type Leaf = NthBooleanLeaf;
    type Node = NthBooleanNode;
    type Slot = NthBooleanSlot;

    #[inline]
    fn fields(&self) -> Result<HashMap<Self::Slot, AnyTerm<'_>>, <Self as Dual>::Leaf> {
        match *self {
            Self::Successor(ref predecessor) => Ok(iter::once((
                NthBooleanSlot::Successor0,
                AnyTerm::new::<Self>(predecessor),
            ))
            .collect()),
            Self::False => Err(NthBooleanLeaf::False),
            Self::True => Err(NthBooleanLeaf::True),
        }
    }

    #[inline]
    fn from_node<F>(node: Self::Node, fields: F) -> Result<Self, DualError>
    where
        F: crate::Fields<Self>,
    {
        Ok(match node {
            NthBooleanNode::Successor => {
                Self::Successor(Box::new(fields.field::<Self>(NthBooleanSlot::Successor0)?))
            }
            NthBooleanNode::False => Self::False,
            NthBooleanNode::True => Self::True,
        })
    }

    #[inline]
    fn register_all_field_types() {
        // you *could* put `Self` here (and, in macros, we should for full generality);
        // it'll just do nothing, since `register` short-circuits on already-registered types.
    }

    #[inline]
    fn slot_type(slot: Self::Slot) -> TypeId {
        match slot {
            NthBooleanSlot::Successor0 => TypeId::of::<Self>(),
        }
    }
}

impl From<NthBooleanBranch> for NthBooleanNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: NthBooleanBranch) -> Self {
        match value {
            NthBooleanBranch::Successor => Self::Successor,
        }
    }
}

impl From<NthBooleanLeaf> for NthBooleanNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: NthBooleanLeaf) -> Self {
        match value {
            NthBooleanLeaf::False => NthBooleanNode::False,
            NthBooleanLeaf::True => NthBooleanNode::True,
        }
    }
}

impl From<NthBooleanSlot> for NthBooleanBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: NthBooleanSlot) -> Self {
        match value {
            NthBooleanSlot::Successor0 => Self::Successor,
        }
    }
}

impl From<NthBooleanBranch> for ErasedBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: NthBooleanBranch) -> Self {
        Self(value as usize)
    }
}

impl From<NthBooleanLeaf> for ErasedLeaf {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: NthBooleanLeaf) -> Self {
        Self(value as usize)
    }
}

impl From<NthBooleanNode> for ErasedNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: NthBooleanNode) -> Self {
        Self(value as usize)
    }
}

impl From<NthBooleanSlot> for ErasedSlot {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: NthBooleanSlot) -> Self {
        Self(value as usize)
    }
}

impl TryFrom<ErasedBranch> for NthBooleanBranch {
    type Error = ErasedBranch;

    #[inline]
    fn try_from(value: ErasedBranch) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::Successor,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedLeaf> for NthBooleanLeaf {
    type Error = ErasedLeaf;

    #[inline]
    fn try_from(value: ErasedLeaf) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            1 => Self::False,
            2 => Self::True,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedNode> for NthBooleanNode {
    type Error = ErasedNode;

    #[inline]
    fn try_from(value: ErasedNode) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::Successor,
            1 => Self::False,
            2 => Self::True,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedSlot> for NthBooleanSlot {
    type Error = ErasedSlot;

    #[inline]
    fn try_from(value: ErasedSlot) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::Successor0,
            _ => return Err(value),
        })
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::DualError, pbt::pbt};

    check_dual_roundtrip!(NthBoolean);

    fn term_coterm_term_roundtrip(term: &NthBoolean) {
        let coterm = Frontier::complete(term);
        let roundtrip: Result<NthBoolean, DualError> = coterm.dual();
        let expected = Ok(term.clone());
        assert_eq!(
            roundtrip, expected,
            "{term:?} -> {coterm:?} -> {roundtrip:?} =/= {expected:?}",
        );
    }
}
