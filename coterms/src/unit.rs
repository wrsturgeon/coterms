//! Implementations for `()`.

use {
    crate::{
        AnyBranch, AnyField, AnyLeaf, AnyNode, AnyTerm, Dual, DualError, ErasedBranch, ErasedField,
        ErasedLeaf, ErasedNode, Fields, HashMap, HashSet, Registry,
    },
    core::{any::TypeId, array::IntoIter, iter::IntoIterator},
};

/// Internal nodes of `()`'s AST.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnitBranch {}

/// Leaves of `()`'s AST.
#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnitLeaf {
    Unit = 0,
}

/// Nodes of `()`'s AST.
#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnitNode {
    Unit = 0,
}

/// Fields of nodes of `()`'s AST.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UnitField {}

impl UnitBranch {
    /// Convert type information into runtime-only data.
    #[inline]
    pub const fn any(self) -> AnyBranch {
        AnyBranch {
            erased: self.erase(),
            ty: TypeId::of::<()>(),
        }
    }

    /// Erase type information.
    #[inline]
    pub const fn erase(self) -> ErasedBranch {
        match self {}
    }

    /// Erase internal/leaf information.
    #[inline]
    pub const fn node(self) -> UnitNode {
        match self {}
    }
}

impl UnitLeaf {
    /// Convert type information into runtime-only data.
    #[inline]
    pub const fn any(self) -> AnyLeaf {
        AnyLeaf {
            erased: self.erase(),
            ty: TypeId::of::<()>(),
        }
    }

    /// Erase type information.
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    pub const fn erase(self) -> ErasedLeaf {
        ErasedLeaf(self as usize)
    }

    /// Erase internal/leaf information.
    #[inline]
    pub const fn node(self) -> UnitNode {
        match self {
            Self::Unit => UnitNode::Unit,
        }
    }
}

impl UnitNode {
    /// Convert type information into runtime-only data.
    #[inline]
    pub const fn any(self) -> AnyNode {
        AnyNode {
            erased: self.erase(),
            ty: TypeId::of::<()>(),
        }
    }

    /// Erase type information.
    #[inline]
    #[expect(clippy::as_conversions, reason = "safe by `repr(..)`")]
    pub const fn erase(self) -> ErasedNode {
        ErasedNode(self as usize)
    }
}

impl UnitField {
    /// Convert type information into runtime-only data.
    #[inline]
    pub const fn any(self) -> AnyField {
        AnyField {
            erased: self.erase(),
            parent_ty: TypeId::of::<()>(),
        }
    }

    /// Which internal node has this field?
    #[inline]
    pub const fn branch(self) -> UnitBranch {
        match self {}
    }

    /// Erase type information.
    #[inline]
    pub const fn erase(self) -> ErasedField {
        match self {}
    }
}

impl Dual for () {
    type Branch = UnitBranch;
    type Deref = Self;
    type Field = UnitField;
    type Leaf = UnitLeaf;
    type Node = UnitNode;

    #[inline]
    fn deref(&self) -> &Self::Deref {
        self
    }

    #[inline]
    fn field_type(field: Self::Field) -> TypeId {
        match field {}
    }

    #[inline]
    fn fields(&self) -> Result<HashMap<Self::Field, AnyTerm<'_>>, Self::Leaf> {
        Err(UnitLeaf::Unit)
    }

    #[inline]
    fn fields_of_node(node: Self::Node) -> Result<HashSet<Self::Field>, Self::Leaf> {
        match node {
            UnitNode::Unit => Err(UnitLeaf::Unit),
        }
    }

    #[inline]
    fn from_node<F>(node: Self::Node, _fields: F) -> Result<Self, DualError>
    where
        F: Fields<Self>,
    {
        match node {
            UnitNode::Unit => Ok(()),
        }
    }

    #[inline]
    fn register_all_field_types(_registry: &mut Registry) {}
}

impl crate::IntoEnumIterator for UnitNode {
    type Iterator = IntoIter<Self, 1>;

    #[inline]
    fn iter() -> Self::Iterator {
        IntoIterator::into_iter([Self::Unit])
    }
}

impl From<UnitBranch> for UnitNode {
    #[inline]
    fn from(value: UnitBranch) -> Self {
        value.node()
    }
}

impl From<UnitLeaf> for UnitNode {
    #[inline]
    fn from(value: UnitLeaf) -> Self {
        value.node()
    }
}

impl From<UnitField> for UnitBranch {
    #[inline]
    fn from(value: UnitField) -> Self {
        value.branch()
    }
}

impl From<UnitBranch> for ErasedBranch {
    #[inline]
    fn from(value: UnitBranch) -> Self {
        value.erase()
    }
}

impl From<UnitLeaf> for ErasedLeaf {
    #[inline]
    fn from(value: UnitLeaf) -> Self {
        value.erase()
    }
}

impl From<UnitNode> for ErasedNode {
    #[inline]
    fn from(value: UnitNode) -> Self {
        value.erase()
    }
}

impl From<UnitField> for ErasedField {
    #[inline]
    fn from(value: UnitField) -> Self {
        value.erase()
    }
}

impl TryFrom<ErasedBranch> for UnitBranch {
    type Error = ErasedBranch;

    #[inline]
    fn try_from(value: ErasedBranch) -> Result<Self, Self::Error> {
        Err(value)
    }
}

impl TryFrom<ErasedLeaf> for UnitLeaf {
    type Error = ErasedLeaf;

    #[inline]
    fn try_from(value: ErasedLeaf) -> Result<Self, Self::Error> {
        match value.0 {
            0 => Ok(Self::Unit),
            _ => Err(value),
        }
    }
}

impl TryFrom<ErasedNode> for UnitNode {
    type Error = ErasedNode;

    #[inline]
    fn try_from(value: ErasedNode) -> Result<Self, Self::Error> {
        match value.0 {
            0 => Ok(Self::Unit),
            _ => Err(value),
        }
    }
}

impl TryFrom<ErasedField> for UnitField {
    type Error = ErasedField;

    #[inline]
    fn try_from(value: ErasedField) -> Result<Self, Self::Error> {
        Err(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{ErasedLeaf, ErasedNode, UnitLeaf, UnitNode};

    #[test]
    fn erased_zero_is_the_unit_leaf_and_node() {
        assert_eq!(UnitLeaf::try_from(ErasedLeaf(0)), Ok(UnitLeaf::Unit));
        assert_eq!(UnitNode::try_from(ErasedNode(0)), Ok(UnitNode::Unit));
    }
}
