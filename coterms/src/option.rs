use {
    crate::{
        AnyNode, Dual, DualError, ErasedLeaf, ErasedNode, ErasedSlot, Frontier, Registry,
        RootedLeaf, RootedPath, any_leaf, any_slot, binary_tree::BinaryTree, typed_node,
    },
    ahash::{HashMap, HashSet, HashSetExt as _},
    alloc::sync::Arc,
    core::{any::TypeId, iter},
    pbt::Pbt,
};

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum OptionLeaf {
    None = 0,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum OptionNode {
    None = 0, // <-- Yes, `Node` needs to include everything, even leaves
    Some = 1,
}

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum OptionSlot {
    Some0,
}

impl<T> Dual for Option<T>
where
    T: Dual,
{
    type Leaf = OptionLeaf;
    type Node = OptionNode;
    type Slot = OptionSlot;

    #[inline]
    fn fields(node: Self::Node) -> Result<HashSet<Self::Slot>, Self::Leaf> {
        match node {
            OptionNode::None => Err(OptionLeaf::None),
            OptionNode::Some => Ok(iter::once(OptionSlot::Some0).collect()),
        }
    }

    #[inline]
    fn from_node(
        nodes: &HashMap<Arc<RootedPath>, AnyNode>,
        path: Arc<RootedPath>,
    ) -> Result<Self, DualError> {
        let Some(index): Option<&AnyNode> = nodes.get(&path) else {
            return Err(DualError::MissingNode(path));
        };
        let node: Self::Node = typed_node::<Self>(index)?;
        Ok(match node {
            OptionNode::None => None,
            #[expect(clippy::todo, reason = "TODO")]
            OptionNode::Some => Some(todo!("we need some dynamic `T::from_node`")),
        })
    }

    #[inline]
    fn register(registry: &mut Registry) {
        registry.register::<T>();
        registry.register::<Self>();
    }

    #[inline]
    fn slot_type(slot: Self::Slot) -> TypeId {
        match slot {
            OptionSlot::Some0 => TypeId::of::<T>(),
        }
    }

    #[inline]
    fn to_leaves(&self, leaves: &mut HashSet<RootedLeaf>, path: Arc<RootedPath>) {
        match *self {
            None => {
                let _: bool = leaves.insert(RootedLeaf {
                    leaf: any_leaf::<Self>(OptionLeaf::None),
                    path,
                });
            }
            Some(ref t) => {
                let () = t.to_leaves(
                    leaves,
                    Arc::new(RootedPath::Step {
                        path,
                        slot: any_slot::<Self>(OptionSlot::Some0),
                    }),
                );
            }
        }
    }
}

impl From<OptionLeaf> for OptionNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: OptionLeaf) -> Self {
        match value {
            OptionLeaf::None => OptionNode::None,
        }
    }
}

impl From<OptionSlot> for OptionNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: OptionSlot) -> Self {
        match value {
            OptionSlot::Some0 => Self::Some,
        }
    }
}

impl From<OptionLeaf> for ErasedLeaf {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: OptionLeaf) -> Self {
        Self(value as usize)
    }
}

impl From<OptionNode> for ErasedNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: OptionNode) -> Self {
        Self(value as usize)
    }
}

impl From<OptionSlot> for ErasedSlot {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: OptionSlot) -> Self {
        Self(value as usize)
    }
}

impl TryFrom<ErasedLeaf> for OptionLeaf {
    type Error = ErasedLeaf;

    #[inline]
    fn try_from(value: ErasedLeaf) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::None,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedNode> for OptionNode {
    type Error = ErasedNode;

    #[inline]
    fn try_from(value: ErasedNode) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::None,
            1 => Self::Some,
            _ => return Err(value),
        })
    }
}

impl TryFrom<ErasedSlot> for OptionSlot {
    type Error = ErasedSlot;

    #[inline]
    fn try_from(value: ErasedSlot) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            0 => Self::Some0,
            _ => return Err(value),
        })
    }
}

#[inline]
#[expect(clippy::ref_option, reason = "consistency")]
fn dual<T>(opt: &Option<T>) -> Frontier
where
    T: Dual,
{
    let mut leaves: HashSet<RootedLeaf> = HashSet::new();
    let () = opt.to_leaves(
        &mut leaves,
        Arc::new(RootedPath::Root {
            ty: TypeId::of::<Option<T>>(),
        }),
    );
    Frontier {
        holes: HashSet::new(),
        leaves,
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{DualError, check_dual_roundtrip},
        pbt::pbt,
    };

    check_dual_roundtrip!(Option<BinaryTree>);

    #[cfg_attr(not(miri), pbt)]
    #[cfg_attr(miri, pbt(100))]
    fn term_coterm_term_roundtrip(term: &Option<BinaryTree>) {
        let coterm = dual(term);
        let roundtrip: Result<Option<BinaryTree>, DualError> = coterm.dual();
        let expected = Ok(term.clone());
        assert_eq!(
            roundtrip, expected,
            "{term:?} -> {coterm:?} -> {roundtrip:?} =/= {expected:?}",
        );
    }
}
