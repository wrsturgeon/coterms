//! Implementations for `Box<_>`.

use {
    crate::{AnyTerm, Dual, DualError, Fields, Registry},
    ahash::{HashMap, HashSet},
    core::any::TypeId,
};

impl<T> Dual for Box<T>
where
    T: Dual,
{
    type Branch = T::Branch;
    type Deref = T::Deref;
    type Field = T::Field;
    type Leaf = T::Leaf;
    type Node = T::Node;

    #[inline]
    fn deref(&self) -> &Self::Deref {
        T::deref(&**self)
    }

    #[inline]
    fn field_type(field: Self::Field) -> TypeId {
        T::field_type(field)
    }

    #[inline]
    fn fields(&self) -> Result<HashMap<Self::Field, AnyTerm<'_>>, Self::Leaf> {
        let deref: &T = self;
        deref.fields()
    }

    #[inline]
    fn fields_of_node(node: Self::Node) -> Result<HashSet<Self::Field>, Self::Leaf> {
        T::fields_of_node(node)
    }

    #[inline]
    fn from_node<F>(node: Self::Node, fields: F) -> Result<Self, DualError>
    where
        F: Fields<Self::Deref>,
    {
        let deref = T::from_node(node, fields)?;
        Ok(Self::new(deref))
    }

    #[inline]
    fn register_all_field_types(registry: &mut Registry) {
        let () = registry.register::<T>();
    }
}

#[cfg(test)]
mod tests {
    crate::check_dual!(Box<bool>);
}
