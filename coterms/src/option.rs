use {
    crate::{
        AnyNode, AnyTerm, Dual, DualError, ErasedBranch, ErasedLeaf, ErasedNode, ErasedSlot,
        Fields, Frontier, Registry, RootedLeaf, RootedPath, any_leaf, any_slot,
        binary_tree::BinaryTree, typed_node,
    },
    ahash::{HashMap, HashSet, HashSetExt as _},
    alloc::sync::Arc,
    core::{
        any::{Any, TypeId},
        iter,
    },
    pbt::Pbt,
};

#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Pbt)]
pub enum OptionBranch {
    Some = 1,
}

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
    type Branch = OptionBranch;
    type Leaf = OptionLeaf;
    type Node = OptionNode;
    type Slot = OptionSlot;

    #[inline]
    fn fields(&self) -> Result<HashMap<Self::Slot, AnyTerm<'_>>, Self::Leaf> {
        match *self {
            None => Err(OptionLeaf::None),
            Some(ref some_0) => {
                Ok(iter::once((OptionSlot::Some0, AnyTerm::new::<T>(some_0))).collect())
            }
        }
    }

    #[inline]
    fn fields_of_node(node: Self::Node) -> Result<HashSet<Self::Slot>, Self::Leaf> {
        match node {
            OptionNode::None => Err(OptionLeaf::None),
            OptionNode::Some => Ok(iter::once(OptionSlot::Some0).collect()),
        }
    }

    #[inline]
    fn from_node<F>(node: Self::Node, fields: F) -> Result<Self, DualError>
    where
        F: Fields<Self>,
    {
        Ok(match node {
            OptionNode::None => None,
            OptionNode::Some => Some(fields.field(OptionSlot::Some0)?),
        })
    }

    #[inline]
    fn register_all_field_types(registry: &mut Registry) {
        let () = registry.register::<T>();
    }

    #[inline]
    fn slot_type(slot: Self::Slot) -> TypeId {
        match slot {
            OptionSlot::Some0 => TypeId::of::<T>(),
        }
    }
}

impl From<OptionBranch> for OptionNode {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: OptionBranch) -> Self {
        match value {
            OptionBranch::Some => OptionNode::Some,
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

impl From<OptionSlot> for OptionBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: OptionSlot) -> Self {
        match value {
            OptionSlot::Some0 => Self::Some,
        }
    }
}

impl From<OptionBranch> for ErasedBranch {
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    fn from(value: OptionBranch) -> Self {
        Self(value as usize)
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

impl TryFrom<ErasedBranch> for OptionBranch {
    type Error = ErasedBranch;

    #[inline]
    fn try_from(value: ErasedBranch) -> Result<Self, Self::Error> {
        Ok(match value.0 {
            1 => Self::Some,
            _ => return Err(value),
        })
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

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{DualError, check_dual},
        pbt::pbt,
    };

    check_dual!(Option<BinaryTree>);
}
