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

/// ADT: 1 + (Self * Self)
#[derive(Clone, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum BinaryTree {
    Branch { lhs: Arc<Self>, rhs: Arc<Self> },
    Leaf,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum BinaryTreeBranch {
    Branch = 1,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum BinaryTreeLeaf {
    Leaf = 0,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum BinaryTreeNode {
    Leaf = 0, // <-- Yes, `Node` needs to include everything, even leaves
    Branch = 1,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum BinaryTreeSlot {
    BranchLhs,
    BranchRhs,
}

impl Dual for BinaryTree {
    type Branch = BinaryTreeBranch;
    type Leaf = BinaryTreeLeaf;
    type Node = BinaryTreeNode;
    type Slot = BinaryTreeSlot;

    #[inline]
    fn fields(&self) -> Result<HashMap<Self::Slot, AnyTerm<'_>>, <Self as Dual>::Leaf> {
        match *self {
            Self::Leaf => Err(BinaryTreeLeaf::Leaf),
            Self::Branch { ref lhs, ref rhs } => Ok([
                (BinaryTreeSlot::BranchLhs, AnyTerm::new::<Self>(lhs)),
                (BinaryTreeSlot::BranchRhs, AnyTerm::new::<Self>(rhs)),
            ]
            .into_iter()
            .collect()),
        }
    }

    #[inline]
    fn fields_of_node(node: Self::Node) -> Result<HashSet<Self::Slot>, <Self as Dual>::Leaf> {
        match node {
            BinaryTreeNode::Leaf => Err(BinaryTreeLeaf::Leaf),
            BinaryTreeNode::Branch => Ok([BinaryTreeSlot::BranchLhs, BinaryTreeSlot::BranchRhs]
                .into_iter()
                .collect()),
        }
    }

    #[inline]
    fn from_node<F>(node: Self::Node, fields: F) -> Result<Self, DualError>
    where
        F: crate::Fields<Self>,
    {
        Ok(match node {
            BinaryTreeNode::Leaf => Self::Leaf,
            BinaryTreeNode::Branch => Self::Branch {
                lhs: Arc::new(fields.field::<Self>(BinaryTreeSlot::BranchLhs)?),
                rhs: Arc::new(fields.field::<Self>(BinaryTreeSlot::BranchRhs)?),
            },
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
            BinaryTreeSlot::BranchLhs | BinaryTreeSlot::BranchRhs => TypeId::of::<Self>(),
        }
    }
}

impl From<BinaryTreeBranch> for BinaryTreeNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BinaryTreeBranch) -> Self {
        match value {
            BinaryTreeBranch::Branch => BinaryTreeNode::Branch,
        }
    }
}

impl From<BinaryTreeLeaf> for BinaryTreeNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BinaryTreeLeaf) -> Self {
        match value {
            BinaryTreeLeaf::Leaf => BinaryTreeNode::Leaf,
        }
    }
}

impl From<BinaryTreeSlot> for BinaryTreeBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BinaryTreeSlot) -> Self {
        match value {
            BinaryTreeSlot::BranchLhs | BinaryTreeSlot::BranchRhs => BinaryTreeBranch::Branch,
        }
    }
}

impl From<BinaryTreeBranch> for ErasedBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BinaryTreeBranch) -> Self {
        Self(value as usize)
    }
}

impl From<BinaryTreeLeaf> for ErasedLeaf {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BinaryTreeLeaf) -> Self {
        Self(value as usize)
    }
}

impl From<BinaryTreeNode> for ErasedNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BinaryTreeNode) -> Self {
        Self(value as usize)
    }
}

impl From<BinaryTreeSlot> for ErasedSlot {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BinaryTreeSlot) -> Self {
        Self(value as usize)
    }
}

impl TryFrom<ErasedBranch> for BinaryTreeBranch {
    type Error = ErasedBranch;

    #[inline]
    fn try_from(value: ErasedBranch) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            1 => Self::Branch,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedLeaf> for BinaryTreeLeaf {
    type Error = ErasedLeaf;

    #[inline]
    fn try_from(value: ErasedLeaf) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::Leaf,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedNode> for BinaryTreeNode {
    type Error = ErasedNode;

    #[inline]
    fn try_from(value: ErasedNode) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::Leaf,
            1 => Self::Branch,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedSlot> for BinaryTreeSlot {
    type Error = ErasedSlot;

    #[inline]
    fn try_from(value: ErasedSlot) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::BranchLhs,
            1 => Self::BranchRhs,
            _ => return Err(value),
        })
    }
}

fn hole_at_root() -> Frontier<BinaryTree> {
    Frontier {
        _phantom: PhantomData,
        holes: [RootedHole {
            path: Arc::new(RootedPath::Root),
            ty: TypeId::of::<BinaryTree>(),
        }]
        .into_iter()
        .collect(),
        leaves: [].into_iter().collect(),
    }
}

fn just_a_leaf() -> Frontier<BinaryTree> {
    Frontier {
        _phantom: PhantomData,
        holes: [].into_iter().collect(),
        leaves: [RootedLeaf {
            leaf: BinaryTreeLeaf::Leaf.into(),
            path: Arc::new(RootedPath::Root),
        }]
        .into_iter()
        .collect(),
    }
}

fn just_a_branch() -> Frontier<BinaryTree> {
    Frontier {
        _phantom: PhantomData,
        holes: [].into_iter().collect(),
        leaves: [
            RootedLeaf {
                leaf: BinaryTreeLeaf::Leaf.into(),
                path: Arc::new(RootedPath::Step {
                    path: Arc::new(RootedPath::Root),
                    slot: BinaryTreeSlot::BranchLhs.into(),
                }),
            },
            RootedLeaf {
                leaf: BinaryTreeLeaf::Leaf.into(),
                path: Arc::new(RootedPath::Step {
                    path: Arc::new(RootedPath::Root),
                    slot: BinaryTreeSlot::BranchRhs.into(),
                }),
            },
        ]
        .into_iter()
        .collect(),
    }
}

fn one_more_branch_on_the_left() -> Frontier<BinaryTree> {
    let left_branch = Arc::new(RootedPath::Step {
        path: Arc::new(RootedPath::Root),
        slot: BinaryTreeSlot::BranchLhs.into(),
    });
    Frontier {
        _phantom: PhantomData,
        holes: [].into_iter().collect(),
        leaves: [
            RootedLeaf {
                leaf: BinaryTreeLeaf::Leaf.into(),
                path: Arc::new(RootedPath::Step {
                    path: Arc::clone(&left_branch),
                    slot: BinaryTreeSlot::BranchLhs.into(),
                }),
            },
            RootedLeaf {
                leaf: BinaryTreeLeaf::Leaf.into(),
                path: Arc::new(RootedPath::Step {
                    path: left_branch,
                    slot: BinaryTreeSlot::BranchRhs.into(),
                }),
            },
            RootedLeaf {
                leaf: BinaryTreeLeaf::Leaf.into(),
                path: Arc::new(RootedPath::Step {
                    path: Arc::new(RootedPath::Root),
                    slot: BinaryTreeSlot::BranchRhs.into(),
                }),
            },
        ]
        .into_iter()
        .collect(),
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{DualError, register},
        pbt::pbt,
    };

    check_dual!(BinaryTree);

    #[test]
    fn dual_leaf() {
        let () = register::<BinaryTree>();
        assert_eq!(Frontier::complete(&BinaryTree::Leaf), just_a_leaf());
    }
}
