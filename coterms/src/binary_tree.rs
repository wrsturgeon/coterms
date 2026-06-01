use {
    crate::{
        Dual, DualError, ErasedNode, ErasedSlot, Filled, Frontier, Path, Place,
        check_dual_roundtrip, root,
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

impl Dual for BinaryTree {
    type Node = BinaryTreeNode;
    type Slot = BinaryTreeSlot;

    #[inline]
    fn from_nodes(nodes: &HashMap<Arc<Place>, ErasedNode>, path: Path) -> Result<Self, DualError> {
        let slot = Arc::new(Place {
            path: path.clone(),
            ty: TypeId::of::<Self>(),
        });
        let Some(&index): Option<&ErasedNode> = nodes.get(&slot) else {
            return Err(DualError::MissingNode(slot));
        };
        let node: Self::Node = index.try_into().map_err(DualError::InvalidNode)?;
        Ok(match node {
            BinaryTreeNode::Leaf => Self::Leaf,
            BinaryTreeNode::Branch => Self::Branch {
                lhs: Arc::new(Self::from_nodes(
                    nodes,
                    Path::Step(Filled {
                        fill: BinaryTreeSlot::BranchLhs.into(),
                        slot: Arc::new(Place {
                            path: path.clone(),
                            ty: TypeId::of::<Self>(),
                        }),
                    }),
                )?),
                rhs: Arc::new(Self::from_nodes(
                    nodes,
                    Path::Step(Filled {
                        fill: BinaryTreeSlot::BranchRhs.into(),
                        slot: Arc::new(Place {
                            path,
                            ty: TypeId::of::<Self>(),
                        }),
                    }),
                )?),
            },
        })
    }

    #[inline]
    fn to_nodes(&self, leaves: &mut HashSet<Filled<ErasedNode>>, path: Path) {
        match *self {
            BinaryTree::Leaf => {
                let _: bool = leaves.insert(Filled {
                    fill: BinaryTreeNode::Leaf.into(),
                    slot: Arc::new(Place {
                        path,
                        ty: TypeId::of::<Self>(),
                    }),
                });
            }
            BinaryTree::Branch { ref lhs, ref rhs } => {
                let () = lhs.to_nodes(
                    leaves,
                    Path::Step(Filled {
                        fill: BinaryTreeSlot::BranchLhs.into(),
                        slot: Arc::new(Place {
                            path: path.clone(),
                            ty: TypeId::of::<Self>(),
                        }),
                    }),
                );
                let () = rhs.to_nodes(
                    leaves,
                    Path::Step(Filled {
                        fill: BinaryTreeSlot::BranchRhs.into(),
                        slot: Arc::new(Place {
                            path,
                            ty: TypeId::of::<Self>(),
                        }),
                    }),
                );
            }
        }
    }

    #[inline]
    fn node(slot: Self::Slot) -> Self::Node {
        match slot {
            BinaryTreeSlot::BranchLhs | BinaryTreeSlot::BranchRhs => BinaryTreeNode::Branch,
        }
    }
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
        holes: [root::<BinaryTree>()].into_iter().collect(),
        leaves: [].into_iter().collect(),
    }
}

fn just_a_leaf() -> Frontier {
    Frontier {
        holes: [].into_iter().collect(),
        leaves: [Filled {
            fill: BinaryTreeNode::Leaf.into(),
            slot: root::<BinaryTree>(),
        }]
        .into_iter()
        .collect(),
    }
}

fn just_a_branch() -> Frontier {
    Frontier {
        holes: [].into_iter().collect(),
        leaves: [
            Filled {
                fill: BinaryTreeNode::Leaf.into(),
                slot: Arc::new(Place {
                    path: Path::Step(Filled {
                        fill: BinaryTreeSlot::BranchLhs.into(),
                        slot: root::<BinaryTree>(),
                    }),
                    ty: TypeId::of::<BinaryTree>(),
                }),
            },
            Filled {
                fill: BinaryTreeNode::Leaf.into(),
                slot: Arc::new(Place {
                    path: Path::Step(Filled {
                        fill: BinaryTreeSlot::BranchRhs.into(),
                        slot: root::<BinaryTree>(),
                    }),
                    ty: TypeId::of::<BinaryTree>(),
                }),
            },
        ]
        .into_iter()
        .collect(),
    }
}

fn one_more_branch_on_the_left() -> Frontier {
    let left_branch = Arc::new(Place {
        path: Path::Step(Filled {
            fill: BinaryTreeSlot::BranchLhs.into(),
            slot: root::<BinaryTree>(),
        }),
        ty: TypeId::of::<BinaryTree>(),
    });
    Frontier {
        holes: [].into_iter().collect(),
        leaves: [
            Filled {
                fill: BinaryTreeNode::Leaf.into(),
                slot: Arc::new(Place {
                    path: Path::Step(Filled {
                        fill: BinaryTreeSlot::BranchLhs.into(),
                        slot: Arc::clone(&left_branch),
                    }),
                    ty: TypeId::of::<BinaryTree>(),
                }),
            },
            Filled {
                fill: BinaryTreeNode::Leaf.into(),
                slot: Arc::new(Place {
                    path: Path::Step(Filled {
                        fill: BinaryTreeSlot::BranchRhs.into(),
                        slot: Arc::clone(&left_branch),
                    }),
                    ty: TypeId::of::<BinaryTree>(),
                }),
            },
            Filled {
                fill: BinaryTreeNode::Leaf.into(),
                slot: Arc::new(Place {
                    path: Path::Step(Filled {
                        fill: BinaryTreeSlot::BranchRhs.into(),
                        slot: root::<BinaryTree>(),
                    }),
                    ty: TypeId::of::<BinaryTree>(),
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
        let mut leaves: HashSet<Filled<ErasedNode>> = HashSet::new();
        let () = self.to_nodes(&mut leaves, Path::Root);
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

    #[pbt]
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
