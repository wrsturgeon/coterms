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
pub enum Peano {
    Zero,
    Successor(Box<Self>),
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum PeanoBranch {
    Successor = 1,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum PeanoLeaf {
    Zero = 0,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum PeanoNode {
    Zero = 0,
    Successor = 1,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum PeanoSlot {
    Successor0 = 1,
}

impl Dual for Peano {
    type Branch = PeanoBranch;
    type Leaf = PeanoLeaf;
    type Node = PeanoNode;
    type Slot = PeanoSlot;

    #[inline]
    fn fields(&self) -> Result<HashMap<Self::Slot, AnyTerm<'_>>, <Self as Dual>::Leaf> {
        match *self {
            Self::Zero => Err(PeanoLeaf::Zero),
            Self::Successor(ref predecessor) => Ok(iter::once((
                PeanoSlot::Successor0,
                AnyTerm::new::<Self>(predecessor),
            ))
            .collect()),
        }
    }

    #[inline]
    fn from_node<F>(node: Self::Node, fields: F) -> Result<Self, DualError>
    where
        F: crate::Fields<Self>,
    {
        Ok(match node {
            PeanoNode::Zero => Self::Zero,
            PeanoNode::Successor => {
                Self::Successor(Box::new(fields.field::<Self>(PeanoSlot::Successor0)?))
            }
        })
    }

    #[inline]
    fn register_all_field_types(_registry: &mut Registry) {
        // you *could* put `Self` here (and, in macros, we should for full generality);
        // it'll just do nothing, since `register` short-circuits on already-registered types.
    }

    #[inline]
    fn slot_type(slot: Self::Slot) -> TypeId {
        match slot {
            PeanoSlot::Successor0 => TypeId::of::<Self>(),
        }
    }
}

impl From<PeanoBranch> for PeanoNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PeanoBranch) -> Self {
        match value {
            PeanoBranch::Successor => Self::Successor,
        }
    }
}

impl From<PeanoLeaf> for PeanoNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PeanoLeaf) -> Self {
        match value {
            PeanoLeaf::Zero => PeanoNode::Zero,
        }
    }
}

impl From<PeanoSlot> for PeanoBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PeanoSlot) -> Self {
        match value {
            PeanoSlot::Successor0 => Self::Successor,
        }
    }
}

impl From<PeanoBranch> for ErasedBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PeanoBranch) -> Self {
        Self(value as usize)
    }
}

impl From<PeanoLeaf> for ErasedLeaf {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PeanoLeaf) -> Self {
        Self(value as usize)
    }
}

impl From<PeanoNode> for ErasedNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PeanoNode) -> Self {
        Self(value as usize)
    }
}

impl From<PeanoSlot> for ErasedSlot {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: PeanoSlot) -> Self {
        Self(value as usize)
    }
}

impl TryFrom<ErasedBranch> for PeanoBranch {
    type Error = ErasedBranch;

    #[inline]
    fn try_from(value: ErasedBranch) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            1 => Self::Successor,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedLeaf> for PeanoLeaf {
    type Error = ErasedLeaf;

    #[inline]
    fn try_from(value: ErasedLeaf) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::Zero,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedNode> for PeanoNode {
    type Error = ErasedNode;

    #[inline]
    fn try_from(value: ErasedNode) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::Zero,
            1 => Self::Successor,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedSlot> for PeanoSlot {
    type Error = ErasedSlot;

    #[inline]
    fn try_from(value: ErasedSlot) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            1 => Self::Successor0,
            _ => return Err(value),
        })
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::DualError, pbt::pbt};

    check_dual_roundtrip!(Peano);

    #[test]
    fn zero_and_one_conflict_at_root() {
        let coterm = Frontier::<Peano> {
            _phantom: PhantomData,
            holes: HashSet::new(),
            leaves: [
                RootedLeaf {
                    leaf: PeanoLeaf::Zero.into(),
                    path: Arc::new(RootedPath::Root),
                },
                RootedLeaf {
                    leaf: PeanoLeaf::Zero.into(),
                    path: Arc::new(RootedPath::Step {
                        path: Arc::new(RootedPath::Root),
                        slot: PeanoSlot::Successor0.into(),
                    }),
                },
            ]
            .into_iter()
            .collect(),
        };

        let decoded = coterm.dual();
        assert!(
            matches!(decoded, Err(DualError::Conflict { .. })),
            "{decoded:?} =/= Err(DualError::Conflict {{ .. }})",
        );
    }

    fn term_coterm_term_roundtrip(term: &Peano) {
        let coterm = Frontier::complete(term);
        let roundtrip: Result<Peano, DualError> = coterm.dual();
        let expected = Ok(term.clone());
        assert_eq!(
            roundtrip, expected,
            "{term:?} -> {coterm:?} -> {roundtrip:?} =/= {expected:?}",
        );
    }
}
