use {
    crate::{
        AnyLeaf, AnyNode, AnySlot, Dual, DualError, ErasedLeaf, ErasedNode, ErasedSlot, Frontier,
        RootedHole, RootedLeaf, RootedPath, any_leaf, any_slot, check_dual_roundtrip, root,
        root_hole, typed_node,
    },
    ahash::{HashMap, HashSet, HashSetExt as _},
    alloc::sync::Arc,
    core::{any::TypeId, iter},
    pbt::Pbt,
};

/// ADT: 1 + (Self * Self)
#[derive(Clone, Debug, Eq, Hash, PartialEq, Pbt)]
enum BinaryTree {
    Branch { lhs: Arc<Self>, rhs: Arc<Self> },
    Leaf,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
enum BinaryTreeLeaf {
    Leaf,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
enum BinaryTreeNode {
    Leaf, // <-- Yes, `Node` needs to include everything, even leaves
    Branch,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
enum BinaryTreeSlot {
    BranchLhs,
    BranchRhs,
}

impl Dual for BinaryTree {
    type Leaf = BinaryTreeLeaf;
    type Node = BinaryTreeNode;
    type Slot = BinaryTreeSlot;

    #[inline]
    fn fields(node: BinaryTreeNode) -> Result<HashSet<AnySlot>, BinaryTreeLeaf> {
        match node {
            BinaryTreeNode::Leaf => Err(BinaryTreeLeaf::Leaf),
            BinaryTreeNode::Branch => Ok([BinaryTreeSlot::BranchLhs, BinaryTreeSlot::BranchRhs]
                .into_iter()
                .map(|slot| AnySlot {
                    index: slot.into(),
                    ty: TypeId::of::<Self>(),
                })
                .collect()),
        }
    }

    #[inline]
    fn from_nodes(
        nodes: &HashMap<Arc<RootedPath>, AnyNode>,
        path: Arc<RootedPath>,
    ) -> Result<Self, DualError> {
        let Some(index): Option<&AnyNode> = nodes.get(&path) else {
            return Err(DualError::MissingNode(path));
        };
        let node: Self::Node = typed_node::<Self>(index)?;
        Ok(match node {
            BinaryTreeNode::Leaf => Self::Leaf,
            BinaryTreeNode::Branch => Self::Branch {
                lhs: Arc::new(Self::from_nodes(
                    nodes,
                    Arc::new(RootedPath::Step {
                        slot: any_slot::<BinaryTree>(BinaryTreeSlot::BranchLhs),
                        path: Arc::clone(&path),
                    }),
                )?),
                rhs: Arc::new(Self::from_nodes(
                    nodes,
                    Arc::new(RootedPath::Step {
                        slot: any_slot::<BinaryTree>(BinaryTreeSlot::BranchRhs),
                        path: Arc::clone(&path),
                    }),
                )?),
            },
        })
    }

    #[inline]
    fn to_leaves(&self, leaves: &mut HashSet<RootedLeaf>, path: Arc<RootedPath>) {
        match *self {
            BinaryTree::Leaf => {
                let _: bool = leaves.insert(RootedLeaf {
                    leaf: any_leaf::<BinaryTree>(BinaryTreeLeaf::Leaf),
                    path,
                });
            }
            BinaryTree::Branch { ref lhs, ref rhs } => {
                let () = lhs.to_leaves(
                    leaves,
                    Arc::new(RootedPath::Step {
                        slot: any_slot::<BinaryTree>(BinaryTreeSlot::BranchLhs),
                        path: Arc::clone(&path),
                    }),
                );
                let () = rhs.to_leaves(
                    leaves,
                    Arc::new(RootedPath::Step {
                        slot: any_slot::<BinaryTree>(BinaryTreeSlot::BranchRhs),
                        path,
                    }),
                );
            }
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

impl From<BinaryTreeSlot> for BinaryTreeNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: BinaryTreeSlot) -> Self {
        match value {
            BinaryTreeSlot::BranchLhs | BinaryTreeSlot::BranchRhs => BinaryTreeNode::Branch,
        }
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

fn hole_at_root() -> Frontier {
    Frontier {
        holes: [root_hole::<BinaryTree>()].into_iter().collect(),
        leaves: [].into_iter().collect(),
    }
}

fn just_a_leaf() -> Frontier {
    Frontier {
        holes: [].into_iter().collect(),
        leaves: [RootedLeaf {
            leaf: AnyLeaf {
                index: BinaryTreeLeaf::Leaf.into(),
                ty: TypeId::of::<BinaryTree>(),
            },
            path: root::<BinaryTree>(),
        }]
        .into_iter()
        .collect(),
    }
}

fn just_a_branch() -> Frontier {
    Frontier {
        holes: [].into_iter().collect(),
        leaves: [
            RootedLeaf {
                leaf: AnyLeaf {
                    index: BinaryTreeLeaf::Leaf.into(),
                    ty: TypeId::of::<BinaryTree>(),
                },
                path: Arc::new(RootedPath::Step {
                    path: root::<BinaryTree>(),
                    slot: any_slot::<BinaryTree>(BinaryTreeSlot::BranchLhs),
                }),
            },
            RootedLeaf {
                leaf: AnyLeaf {
                    index: BinaryTreeLeaf::Leaf.into(),
                    ty: TypeId::of::<BinaryTree>(),
                },
                path: Arc::new(RootedPath::Step {
                    path: root::<BinaryTree>(),
                    slot: any_slot::<BinaryTree>(BinaryTreeSlot::BranchRhs),
                }),
            },
        ]
        .into_iter()
        .collect(),
    }
}

fn one_more_branch_on_the_left() -> Frontier {
    let left_branch = Arc::new(RootedPath::Step {
        path: root::<BinaryTree>(),
        slot: any_slot::<BinaryTree>(BinaryTreeSlot::BranchLhs),
    });
    Frontier {
        holes: [].into_iter().collect(),
        leaves: [
            RootedLeaf {
                leaf: AnyLeaf {
                    index: BinaryTreeLeaf::Leaf.into(),
                    ty: TypeId::of::<BinaryTree>(),
                },
                path: Arc::new(RootedPath::Step {
                    path: Arc::clone(&left_branch),
                    slot: any_slot::<BinaryTree>(BinaryTreeSlot::BranchLhs),
                }),
            },
            RootedLeaf {
                leaf: AnyLeaf {
                    index: BinaryTreeLeaf::Leaf.into(),
                    ty: TypeId::of::<BinaryTree>(),
                },
                path: Arc::new(RootedPath::Step {
                    path: left_branch,
                    slot: any_slot::<BinaryTree>(BinaryTreeSlot::BranchRhs),
                }),
            },
            RootedLeaf {
                leaf: AnyLeaf {
                    index: BinaryTreeLeaf::Leaf.into(),
                    ty: TypeId::of::<BinaryTree>(),
                },
                path: Arc::new(RootedPath::Step {
                    path: root::<BinaryTree>(),
                    slot: any_slot::<BinaryTree>(BinaryTreeSlot::BranchRhs),
                }),
            },
        ]
        .into_iter()
        .collect(),
    }
}

impl BinaryTree {
    #[inline]
    fn dual(&self) -> Frontier {
        let mut leaves: HashSet<RootedLeaf> = HashSet::new();
        let () = self.to_leaves(&mut leaves, root::<Self>());
        Frontier {
            holes: HashSet::new(),
            leaves,
        }
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::DualError, pbt::pbt};

    check_dual_roundtrip!(BinaryTree);

    #[test]
    fn dual_leaf() {
        assert_eq!(BinaryTree::Leaf.dual(), just_a_leaf());
    }

    #[cfg_attr(not(miri), pbt)]
    #[cfg_attr(miri, pbt(100))]
    fn term_coterm_term_roundtrip(term: &BinaryTree) {
        let coterm = term.dual();
        let roundtrip: Result<BinaryTree, DualError> = coterm.dual();
        let expected = Ok(term.clone());
        assert_eq!(
            roundtrip, expected,
            "{term:?} -> {coterm:?} -> {roundtrip:?} =/= {expected:?}",
        );
    }
}
