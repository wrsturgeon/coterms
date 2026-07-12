//! Top-down node-by-node construction and incremental computing.

extern crate alloc;

#[cfg(test)]
extern crate self as coterms;

mod boolean;
mod boxes;
mod unit;

pub use {
    ahash::{HashMap, HashMapExt, HashSet, HashSetExt},
    coterms_macros::*,
    strum::IntoEnumIterator,
};

use {
    ahash::RandomState,
    alloc::sync::Arc,
    core::{
        any::{self, TypeId},
        fmt,
        hash::{BuildHasher, Hash},
        iter,
        marker::PhantomData,
        ptr,
    },
    pbt::Pbt,
    std::{
        collections::{HashMap as StdHashMap, hash_map},
        sync::RwLock,
    },
};

/// The process-wide dispatch table used for type-erased [`Dual`] operations.
///
/// Prefer [`register`] over mutating this registry directly.
pub static REGISTRY: RwLock<Registry> = RwLock::new(Registry {
    #[expect(clippy::unusual_byte_groupings, reason = "readability")]
    dispatch: HashMap::with_hasher(RandomState::with_seeds(
        0xBAAD_5EED_BAAD_C0DE,
        0xC0DE_CAFE_DECAF_BAD,
        0xDEFEC8ED__BAAD_D00D,
        0x1337_1337_1337_1337,
    )),
});

/// Describes a term type that can be decomposed into, and rebuilt from, typed nodes.
///
/// Implementations provide compact enum-like representations of branches, leaves,
/// fields, and nodes. The crate erases those representations to traverse terms whose
/// concrete field types vary recursively.
pub trait Dual: 'static + Clone {
    /// A non-leaf node constructor shared by all of the branch's fields.
    type Branch: Copy
        + fmt::Debug
        + Into<ErasedBranch>
        + TryFrom<ErasedBranch, Error = ErasedBranch>
        + Into<Self::Node>;
    /// The canonical term type represented by references to `Self`.
    type Deref: Dual<
            Branch = Self::Branch,
            Leaf = Self::Leaf,
            Node = Self::Node,
            Field = Self::Field,
            Deref = Self::Deref,
        >;
    /// A field selecting one child of a branch.
    type Field: Copy
        + fmt::Debug
        + Eq
        + Hash
        + Into<ErasedField>
        + TryFrom<ErasedField, Error = ErasedField>
        + Into<Self::Branch>;
    /// A node constructor with no child fields.
    type Leaf: Copy
        + fmt::Debug
        + Into<ErasedLeaf>
        + TryFrom<ErasedLeaf, Error = ErasedLeaf>
        + Into<Self::Node>;
    /// Any node constructor, encompassing both branches and leaves.
    type Node: Copy
        + fmt::Debug
        + Into<ErasedNode>
        + IntoEnumIterator
        + TryFrom<ErasedNode, Error = ErasedNode>;
    /// Borrows this value as its canonical term type.
    fn deref(&self) -> &Self::Deref;
    /// The type with which this field should be filled.
    fn field_type(field: Self::Field) -> TypeId;
    /// All fields of this value, or a statically known leaf if there are none.
    ///
    /// # Errors
    ///
    /// If there are no fields (in this case, this safely returns a leaf).
    fn fields(&self) -> Result<HashMap<Self::Field, AnyTerm<'_>>, Self::Leaf>;
    // TODO: should the below be a map that's cached at runtime
    // by scanning and using the `Into` trait to construct fibers?
    /// All fields of a given node, or a statically known leaf if there are none.
    ///
    /// # Errors
    ///
    /// If there are no fields (in this case, this safely returns a leaf).
    fn fields_of_node(node: Self::Node) -> Result<HashSet<Self::Field>, Self::Leaf>;
    /// Reconstruct this type from its AST.
    ///
    /// # Errors
    ///
    /// If the AST is not well-formed with respect to this type.
    fn from_node<F>(node: Self::Node, fields: F) -> Result<Self, DualError>
    where
        F: Fields<Self::Deref>;
    /// Registers every term type that may occur in one of this type's fields.
    fn register_all_field_types(registry: &mut Registry);
}

/// A type-erased branch tagged with the [`TypeId`] of its parent term.
#[expect(
    clippy::exhaustive_structs,
    reason = "intentionally minimal and stable"
)]
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AnyBranch {
    /// The type-erased branch discriminant.
    pub erased: ErasedBranch,
    /// The canonical parent term type.
    pub ty: TypeId,
}

/// A type-erased leaf tagged with the [`TypeId`] of its term.
#[expect(
    clippy::exhaustive_structs,
    reason = "intentionally minimal and stable"
)]
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AnyLeaf {
    /// The type-erased leaf discriminant.
    pub erased: ErasedLeaf,
    /// The canonical term type.
    pub ty: TypeId,
}

/// A type-erased node tagged with the [`TypeId`] of its term.
#[expect(
    clippy::exhaustive_structs,
    reason = "intentionally minimal and stable"
)]
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AnyNode {
    /// The type-erased node discriminant.
    pub erased: ErasedNode,
    /// The canonical term type.
    pub ty: TypeId,
}

/// A type-erased field tagged with the [`TypeId`] of its parent term.
#[expect(
    clippy::exhaustive_structs,
    reason = "intentionally minimal and stable"
)]
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AnyField {
    /// The type-erased field discriminant.
    pub erased: ErasedField,
    /// The canonical type of the term containing this field.
    pub parent_ty: TypeId,
}

/// A borrowed, type-erased term tagged with its canonical [`TypeId`].
#[non_exhaustive]
#[derive(Debug)]
pub struct AnyTerm<'term> {
    /// The erased pointer whose lifetime is tied to this wrapper.
    pub erased: ErasedTerm<'term>,
    /// The canonical type of the pointed-to term.
    pub ty: TypeId,
}

/// For testing only. Implements `Fields` by cloning pre-existing fields.
#[cfg(test)]
struct CloneFields<'field, D>
where
    D: Dual,
{
    fields: HashMap<D::Field, AnyTerm<'field>>,
}

#[non_exhaustive]
/// Runtime operations for one registered canonical term type.
pub struct Conversions {
    /// Converts an erased branch discriminant into an erased node discriminant.
    pub branch: fn(ErasedBranch) -> Result<ErasedNode, DualError>,
    /// Converts an erased field discriminant into its branch discriminant.
    pub field: fn(ErasedField) -> Result<ErasedBranch, DualError>,
    /// Returns the canonical type accepted by an erased field.
    pub field_type: fn(ErasedField) -> Result<TypeId, DualError>,
    /// Decomposes a borrowed term into erased fields, or returns its leaf.
    pub fields: for<'term> fn(
        ErasedTerm<'term>,
    ) -> Result<HashMap<ErasedField, AnyTerm<'term>>, ErasedLeaf>,
    /// Returns a node's erased fields, or its leaf, after validating the node.
    pub fields_of_node:
        for<'term> fn(ErasedNode) -> Result<Result<HashSet<ErasedField>, ErasedLeaf>, DualError>,
    /// Converts an erased leaf discriminant into an erased node discriminant.
    pub leaf: fn(ErasedLeaf) -> Result<ErasedNode, DualError>,
    /// The diagnostic name of the registered canonical term type.
    pub type_name: &'static str,
}

/// An error produced while validating, decomposing, or rebuilding a dual term.
#[non_exhaustive]
#[derive(Debug, Eq, PartialEq)]
pub enum DualError {
    /// Two different nodes were assigned to the same rooted path.
    Conflict {
        /// The path receiving both assignments.
        at: Arc<RootedPath>,
        /// The node assigned first.
        existing: AnyNode,
        /// The conflicting node assigned later.
        incoming: AnyNode,
    },
    /// A frontier cannot be rebuilt because it still contains holes.
    Incomplete {
        /// The unfilled paths and the canonical types they require.
        holes: HashMap<Arc<RootedPath>, TypeId>,
    },
    /// An erased branch is not valid for its tagged term type.
    InvalidBranch(AnyBranch),
    /// An erased field is not valid for its tagged parent type.
    InvalidField(AnyField),
    /// An erased leaf is not valid for its tagged term type.
    InvalidLeaf(AnyLeaf),
    /// An erased node is not valid for its tagged term type.
    InvalidNode(AnyNode),
    /// A term reconstruction requested a field that was not supplied.
    MissingField {
        /// The missing field, relative to the term being reconstructed.
        local: AnyField,
    },
    /// No node was supplied at a required rooted path.
    MissingNode {
        /// The path at which a node was required.
        at: Arc<RootedPath>,
    },
    /// A field was used with a different child type than it accepts.
    MistypedField {
        /// The field whose child type did not match.
        actual: AnyField,
        /// The required canonical child type.
        expected: TypeId,
    },
    /// A leaf was tagged with a different canonical type than expected.
    MistypedLeaf {
        /// The incorrectly tagged leaf.
        actual: AnyLeaf,
        /// The required canonical type.
        expected: TypeId,
    },
    /// A node was tagged with a different canonical type than expected.
    MistypedNode {
        /// The incorrectly tagged node.
        actual: AnyNode,
        /// The required canonical type.
        expected: TypeId,
    },
    /// A requested path is not an unfilled hole in the frontier.
    NoSuchHole(Arc<RootedPath>),
    /// No runtime conversions have been registered for the given type.
    UnregisteredType(TypeId),
}

#[repr(transparent)]
#[expect(clippy::exhaustive_structs, reason = "genuinely exhaustive")]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
/// A type-erased branch discriminant.
pub struct ErasedBranch(pub usize);

#[repr(transparent)]
#[expect(clippy::exhaustive_structs, reason = "genuinely exhaustive")]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
/// A type-erased leaf discriminant.
pub struct ErasedLeaf(pub usize);

#[repr(transparent)]
#[expect(clippy::exhaustive_structs, reason = "genuinely exhaustive")]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
/// A type-erased node discriminant.
pub struct ErasedNode(pub usize);

#[repr(transparent)]
#[expect(clippy::exhaustive_structs, reason = "genuinely exhaustive")]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
/// A type-erased field discriminant.
pub struct ErasedField(pub usize);

#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
/// A type-erased pointer to a term borrowed for `'term`.
pub struct ErasedTerm<'term> {
    /// Prevents the erased pointer from outliving its source term.
    lifetime: PhantomData<&'term ()>,
    /// Points to the canonical term value identified by the enclosing type tag.
    ptr: *const (),
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// An incrementally constructed term represented by typed holes and completed leaves.
pub struct Frontier<D>
where
    D: Dual,
{
    /// Associates this erased representation with its root term type.
    _phantom: PhantomData<D>,
    /// Unfilled paths paired with the canonical term types they accept.
    holes: HashMap<Arc<RootedPath>, TypeId>,
    /// Completed leaf paths paired with their erased leaf discriminants.
    leaves: HashMap<Arc<RootedPath>, ErasedLeaf>,
}

/// Reconstructs fields by lazily reading nodes from an in-progress pin-up.
struct LazyFields<'pinup> {
    /// The path of the term currently being reconstructed.
    path: Arc<RootedPath>,
    /// All nodes pinned so far by rooted path.
    pinup: &'pinup HashMap<Arc<RootedPath>, AnyNode>,
}

/// Reconstructs fields from a complete externally supplied pinned map.
struct PinnedFields<'pinned, S> {
    /// The path of the term currently being reconstructed.
    path: Arc<RootedPath>,
    /// The complete map from rooted paths to nodes.
    pinned: &'pinned StdHashMap<Arc<RootedPath>, AnyNode, S>,
}

/// For testing only. Implements `Fields` but panics on `.field(..)`.
#[cfg(test)]
struct NoFields;

#[non_exhaustive]
/// Runtime dispatch entries indexed by canonical term [`TypeId`].
pub struct Registry {
    /// Conversion operations for each registered canonical term type.
    pub dispatch: HashMap<TypeId, Conversions>,
}

/// An unfilled rooted path paired with the canonical term type it accepts.
#[non_exhaustive]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RootedHole {
    /// The location of the hole in the term.
    pub path: Arc<RootedPath>,
    /// The canonical term type accepted at that location.
    pub ty: TypeId,
}

/// A type-erased path from some node
/// all the way up to the global root.
#[expect(clippy::exhaustive_enums, reason = "intentionally minimal and stable")]
#[derive(Clone, Eq, Hash, PartialEq, Pbt)]
pub enum RootedPath {
    /// The root term itself.
    Root,
    /// A child reached through one field of its parent.
    Step {
        /// The rooted path of the parent term.
        path: Arc<Self>,
        /// The particular field into which this value is inserted.
        field: ErasedField,
    },
}

/// The branch discriminant associated with a [`Dual`] type.
pub type BranchOf<D> = <D as Dual>::Branch;

/// The field discriminant associated with a [`Dual`] type.
pub type FieldOf<D> = <D as Dual>::Field;

/// The leaf discriminant associated with a [`Dual`] type.
pub type LeafOf<D> = <D as Dual>::Leaf;

/// The node discriminant associated with a [`Dual`] type.
pub type NodeOf<D> = <D as Dual>::Node;

/// Supplies typed child fields while reconstructing a [`Dual`] term.
pub trait Fields<D>
where
    D: Dual,
{
    /// Reconstructs the child stored in `field`.
    ///
    /// # Errors
    ///
    /// Returns [`DualError`] if the field is absent, has the wrong type, or its
    /// stored subtree cannot be reconstructed.
    fn field<T>(&self, field: D::Field) -> Result<T, DualError>
    where
        T: Dual;
}

impl fmt::Debug for AnyBranch {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[expect(
            clippy::unwrap_in_result,
            clippy::unwrap_used,
            reason = "a poisoned lock means another panic already occurred"
        )]
        let registry = REGISTRY.read().unwrap();
        if let Some(conversions) = registry.dispatch.get(&self.ty) {
            write!(f, "<{}>::branch#{}", conversions.type_name, self.erased.0)
        } else {
            f.debug_struct("AnyBranch")
                .field("ty", &self.ty)
                .field("erased", &self.erased)
                .finish()
        }
    }
}

impl fmt::Debug for AnyLeaf {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[expect(
            clippy::unwrap_in_result,
            clippy::unwrap_used,
            reason = "a poisoned lock means another panic already occurred"
        )]
        let registry = REGISTRY.read().unwrap();
        if let Some(conversions) = registry.dispatch.get(&self.ty)
            && let Ok(_node) = (conversions.leaf)(self.erased)
        {
            write!(f, "<{}>::leaf#{}", conversions.type_name, self.erased.0)
        } else {
            f.debug_struct("AnyLeaf")
                .field("ty", &self.ty)
                .field("erased", &self.erased)
                .finish()
        }
    }
}

impl fmt::Debug for AnyNode {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[expect(
            clippy::unwrap_in_result,
            clippy::unwrap_used,
            reason = "a poisoned lock means another panic already occurred"
        )]
        let registry = REGISTRY.read().unwrap();
        if let Some(conversions) = registry.dispatch.get(&self.ty) {
            write!(f, "<{}>::node#{}", conversions.type_name, self.erased.0)
        } else {
            f.debug_struct("AnyNode")
                .field("ty", &self.ty)
                .field("erased", &self.erased)
                .finish()
        }
    }
}

impl fmt::Debug for AnyField {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        #[expect(
            clippy::unwrap_in_result,
            clippy::unwrap_used,
            reason = "a poisoned lock means another panic already occurred"
        )]
        let registry = REGISTRY.read().unwrap();
        if let Some(conversions) = registry.dispatch.get(&self.parent_ty) {
            write!(f, "<{}>::field#{}", conversions.type_name, self.erased.0)
        } else {
            f.debug_struct("AnyField")
                .field("parent_ty", &self.parent_ty)
                .field("erased", &self.erased)
                .finish()
        }
    }
}

impl fmt::Debug for RootedPath {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Root => f.write_str("Root"),
            Self::Step { ref path, field } => {
                let () = fmt::Debug::fmt(path, f)?;
                let () = f.write_str(".")?;
                fmt::Debug::fmt(&field.0, f)
            }
        }
    }
}

#[cfg(test)]
impl<D> Fields<D> for CloneFields<'_, D>
where
    D: Dual,
{
    #[inline]
    fn field<T>(&self, field: D::Field) -> Result<T, DualError>
    where
        T: Dual,
    {
        let any_term: &AnyTerm<'_> =
            self.fields
                .get(&field)
                .ok_or_else(|| DualError::MissingField {
                    local: any_field::<D>(field),
                })?;
        let ty = TypeId::of::<T::Deref>();
        if any_term.ty != ty {
            return Err(DualError::MistypedField {
                actual: any_field::<D>(field),
                expected: any_term.ty,
            });
        }
        // SAFETY: Assuming `AnyTerm` has been soundly constructed,
        // this reinterpretation is valid b/c `ty == ty` above.
        let deref: &T::Deref = unsafe { any_term.erased.ptr.cast::<T::Deref>().as_ref_unchecked() };
        let pinned = pin_up(deref)?;
        from_pinned(&pinned)
    }
}

impl AnyField {
    /// Returns the canonical type accepted by this field.
    ///
    /// # Errors
    ///
    /// Returns [`DualError::UnregisteredType`] if the parent type is not
    /// registered, or [`DualError::InvalidField`] if the erased discriminant is
    /// not a field of that parent.
    ///
    /// # Panics
    ///
    /// Panics if the global [`REGISTRY`] lock is poisoned.
    #[inline]
    pub fn field_type(&self) -> Result<TypeId, DualError> {
        #[expect(
            clippy::unwrap_in_result,
            clippy::unwrap_used,
            reason = "a poisoned lock means another panic already occurred"
        )]
        let registry = REGISTRY.read().unwrap();
        let conversions = registry
            .dispatch
            .get(&self.parent_ty)
            .ok_or(DualError::UnregisteredType(self.parent_ty))?;
        (conversions.field_type)(self.erased)
    }
}

impl<'term> AnyTerm<'term> {
    /// Erases a borrowed term while retaining its canonical type and lifetime.
    #[inline]
    pub fn new<D>(term: &'term D) -> Self
    where
        D: Dual,
    {
        let deref = D::deref(term);
        Self {
            erased: ErasedTerm {
                lifetime: PhantomData,
                ptr: ptr::from_ref(deref).cast(),
            },
            ty: TypeId::of::<D::Deref>(),
        }
    }
}

impl<D> Frontier<D>
where
    D: Dual,
{
    /// Captures a complete term as a frontier with no remaining holes.
    ///
    /// # Panics
    ///
    /// Panics if the global [`REGISTRY`] lock is poisoned or if a [`Dual`]
    /// implementation produces an internally inconsistent decomposition.
    #[inline]
    pub fn complete(d: &D) -> Self {
        let () = register::<D>();
        #[expect(
            clippy::unwrap_used,
            reason = "a poisoned lock means another panic already occurred"
        )]
        let registry = REGISTRY.read().unwrap();
        let mut leaves = HashMap::new();
        #[expect(
            clippy::expect_used,
            reason = "all possible failures here are internal and ought to fail loudly"
        )]
        let () = Self::complete_leaves(
            &AnyTerm::new(d),
            Arc::new(RootedPath::Root),
            &mut leaves,
            &registry,
        )
        .expect("INTERNAL ERROR (`coterms`)");
        Self {
            _phantom: PhantomData,
            holes: HashMap::new(),
            leaves,
        }
    }

    /// Recursively records every leaf below `term`.
    #[inline]
    fn complete_leaves(
        term: &AnyTerm<'_>,
        path: Arc<RootedPath>,
        leaves: &mut HashMap<Arc<RootedPath>, ErasedLeaf>,
        registry: &Registry,
    ) -> Result<(), DualError> {
        let () = match registry.fields(term)? {
            Err(leaf) => {
                let _: Option<_> = leaves.insert(path, leaf);
                // TODO: do we need to check for consistency here?
            }
            Ok(fields) =>
            {
                #[expect(clippy::iter_over_hash_type, reason = "order doesn't matter")]
                for (field, value) in fields {
                    let extended_path = Arc::new(RootedPath::Step {
                        path: Arc::clone(&path),
                        field,
                    });
                    let () = Self::complete_leaves(&value, extended_path, leaves, registry)?;
                }
            }
        };
        Ok(())
    }

    /// Rebuilds the root term after every frontier hole has been filled.
    ///
    /// # Errors
    ///
    /// Returns [`DualError::Incomplete`] while holes remain. Other variants
    /// report unregistered types, invalid erased values, conflicting nodes, or
    /// failures from the root type's [`Dual::from_node`] implementation.
    ///
    /// # Panics
    ///
    /// Panics if the global [`REGISTRY`] lock is poisoned.
    #[inline]
    #[expect(
        clippy::unwrap_in_result,
        reason = "a poisoned lock means another panic already occurred"
    )]
    pub fn dual(&self) -> Result<D, DualError> {
        let () = register::<D>();
        #[expect(
            clippy::unwrap_used,
            reason = "a poisoned lock means another panic already occurred"
        )]
        let registry = REGISTRY.read().unwrap();
        if !self.holes.is_empty() {
            return Err(DualError::Incomplete {
                holes: self.holes.clone(),
            });
        }
        // TODO: This `HashMap` shouldn't be necessary at all if we hash-cons internal structure!
        let mut pinup: HashMap<Arc<RootedPath>, AnyNode> = HashMap::new();
        #[expect(clippy::iter_over_hash_type, reason = "order doesn't matter")]
        for (path, &leaf) in &self.leaves {
            let parent_type = Self::pin_up(path, &mut pinup, &registry)?;
            let conversions = registry
                .dispatch
                .get(&parent_type)
                .ok_or(DualError::UnregisteredType(parent_type))?;
            let node = AnyNode {
                erased: (conversions.leaf)(leaf)?,
                ty: parent_type,
            };
            let () = Self::pin_node(&mut pinup, Arc::clone(path), node)?;
        }
        let root_path = Arc::new(RootedPath::Root);
        let Some(root_any_node) = pinup.get(&root_path) else {
            return Err(DualError::MissingNode { at: root_path });
        };
        let root_node: D::Node = typed_node::<D>(root_any_node)?;
        D::from_node(
            root_node,
            LazyFields {
                path: root_path,
                pinup: &pinup,
            },
        )
    }

    /// Fills one frontier hole with a node and opens any fields of that node.
    ///
    /// The returned set contains the fields that became new holes. A leaf
    /// returns an empty set.
    ///
    /// # Errors
    ///
    /// Returns [`DualError::NoSuchHole`] if `hole` is not currently open.
    /// Other variants report an unregistered type, an invalid node or field,
    /// or a type mismatch.
    ///
    /// # Panics
    ///
    /// Panics if the global [`REGISTRY`] lock is poisoned.
    #[inline]
    #[expect(
        clippy::unwrap_in_result,
        reason = "a poisoned lock means another panic already occurred"
    )]
    pub fn fill(
        &mut self,
        hole: &Arc<RootedPath>,
        node: ErasedNode,
    ) -> Result<HashSet<ErasedField>, DualError> {
        let () = register::<D>();
        let Some(hole_ty) = self.holes.remove(hole) else {
            return Err(DualError::NoSuchHole(Arc::clone(hole)));
        };
        #[expect(
            clippy::unwrap_used,
            reason = "a poisoned lock means another panic already occurred"
        )]
        let registry = REGISTRY.read().unwrap();
        let conversions = registry
            .dispatch
            .get(&hole_ty)
            .ok_or(DualError::UnregisteredType(hole_ty))?;
        let node_fields: Result<HashSet<ErasedField>, ErasedLeaf> =
            (conversions.fields_of_node)(node)?;
        match node_fields {
            Err(leaf) => {
                let _dup: Option<ErasedLeaf> = self.leaves.insert(Arc::clone(hole), leaf);
                // TODO: do we need to check for consistency here?
                // TODO: should we use a `HashMap<Arc<RootedPath>, ErasedLeaf>` instead?
                Ok(HashSet::new())
            }
            Ok(fields) => {
                #[expect(clippy::iter_over_hash_type, reason = "order doesn't matter")]
                for &field in &fields {
                    let _dup: Option<TypeId> = self.holes.insert(
                        Arc::new(RootedPath::Step {
                            path: Arc::clone(hole),
                            field,
                        }),
                        (conversions.field_type)(field)?,
                    );
                    // TODO: do we need to check for consistency here?
                    // TODO: should we use a `HashMap<Arc<RootedPath>, TypeId>` instead?
                }
                Ok(fields)
            }
        }
    }

    /// Creates a frontier whose only hole is the root term.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        let () = register::<D>();
        Self {
            holes: iter::once((Arc::new(RootedPath::Root), TypeId::of::<D::Deref>())).collect(),
            leaves: HashMap::new(),
            _phantom: PhantomData,
        }
    }

    /// Inserts a node at `path`, rejecting a conflicting prior assignment.
    #[inline]
    fn pin_node(
        pinup: &mut HashMap<Arc<RootedPath>, AnyNode>,
        path: Arc<RootedPath>,
        node: AnyNode,
    ) -> Result<(), DualError> {
        match pinup.entry(Arc::clone(&path)) {
            hash_map::Entry::Vacant(vacant) => {
                let _: &mut AnyNode = vacant.insert(node);
                Ok(())
            }
            hash_map::Entry::Occupied(occupied) => {
                let existing = occupied.get();
                if *existing == node {
                    Ok(())
                } else {
                    Err(DualError::Conflict {
                        at: path,
                        existing: *existing,
                        incoming: node,
                    })
                }
            }
        }
    }

    /// Synthesizes ancestor branch nodes and returns the type required at `path`.
    #[inline]
    fn pin_up(
        path: &RootedPath,
        pinup: &mut HashMap<Arc<RootedPath>, AnyNode>,
        registry: &Registry,
    ) -> Result<TypeId, DualError> {
        let RootedPath::Step {
            path: ref parent_path,
            field,
        } = *path
        else {
            return Ok(TypeId::of::<D::Deref>());
        };
        let parent_type = if let Some(existing) = pinup.get(parent_path) {
            existing.ty
        } else {
            Self::pin_up(parent_path, pinup, registry)?
        };
        let conversions = registry
            .dispatch
            .get(&parent_type)
            .ok_or(DualError::UnregisteredType(parent_type))?;
        let () = Self::pin_node(
            pinup,
            Arc::clone(parent_path),
            AnyNode {
                erased: (conversions.branch)((conversions.field)(field)?)?,
                ty: parent_type,
            },
        )?;
        (conversions.field_type)(field)
    }
}

impl<D> Default for Frontier<D>
where
    D: Dual,
{
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<D> Fields<D> for LazyFields<'_>
where
    D: Dual,
{
    #[inline]
    fn field<T>(&self, field: D::Field) -> Result<T, DualError>
    where
        T: Dual,
    {
        let path = Arc::new(RootedPath::Step {
            path: Arc::clone(&self.path),
            field: field.into(),
        });
        let Some(any_node) = self.pinup.get(&path) else {
            return Err(DualError::MissingNode { at: path });
        };
        let node: T::Node = typed_node::<T>(any_node)?;
        T::from_node(
            node,
            Self {
                path,
                pinup: self.pinup,
            },
        )
    }
}

impl<D, S> Fields<D> for PinnedFields<'_, S>
where
    D: Dual,
    S: BuildHasher,
{
    #[inline]
    fn field<T>(&self, field: D::Field) -> Result<T, DualError>
    where
        T: Dual,
    {
        let expected = TypeId::of::<T::Deref>();
        if D::field_type(field) != expected {
            return Err(DualError::MistypedField {
                actual: any_field::<D>(field),
                expected,
            });
        }
        let path = Arc::new(RootedPath::Step {
            path: Arc::clone(&self.path),
            field: field.into(),
        });
        let Some(any_node) = self.pinned.get(&path) else {
            return Err(DualError::MissingNode { at: path });
        };
        let node: T::Node = typed_node::<T>(any_node)?;
        T::from_node(
            node,
            Self {
                path,
                pinned: self.pinned,
            },
        )
    }
}

#[cfg(test)]
impl<D> Fields<D> for NoFields
where
    D: Dual,
{
    #[inline]
    #[expect(
        clippy::panic,
        clippy::panic_in_result_fn,
        reason = "for testing only (note `#[cfg(test)]`)"
    )]
    fn field<T>(&self, _field: D::Field) -> Result<T, DualError>
    where
        T: Dual,
    {
        panic!("Called `.field(..)` on `NoFields`")
    }
}

impl Registry {
    /// Decomposes an erased borrowed term using its registered conversions.
    #[inline]
    fn fields<'term>(
        &self,
        &AnyTerm { erased, ty }: &AnyTerm<'term>,
    ) -> Result<Result<HashMap<ErasedField, AnyTerm<'term>>, ErasedLeaf>, DualError> {
        let f = self
            .dispatch
            .get(&ty)
            .ok_or(DualError::UnregisteredType(ty))?
            .fields;
        Ok(f(erased))
    }

    /// Converts a tagged erased leaf into a tagged erased node.
    #[inline]
    fn leaf(&self, &AnyLeaf { erased, ty }: &AnyLeaf) -> Result<AnyNode, DualError> {
        let f = self
            .dispatch
            .get(&ty)
            .ok_or(DualError::UnregisteredType(ty))?
            .leaf;
        Ok(AnyNode {
            erased: f(erased)?,
            ty,
        })
    }

    /// Adds the conversions for `D` and recursively registers its field types.
    #[inline]
    pub fn register<D>(&mut self)
    where
        D: Dual,
    {
        let ty = TypeId::of::<D>();
        if self.dispatch.contains_key(&ty) {
            return;
        }
        let _: Option<_> = self.dispatch.insert(
            ty,
            Conversions {
                branch: |erased_branch| {
                    let branch: D::Branch = erased_branch.try_into().map_err(|erased| {
                        DualError::InvalidBranch(AnyBranch {
                            erased,
                            ty: TypeId::of::<D>(),
                        })
                    })?;
                    let node: D::Node = branch.into();
                    Ok(node.into())
                },
                fields: |ErasedTerm { ptr, .. }| {
                    // SAFETY: `AnyTerm::new` stores a pointer tagged with `D::Deref`, and
                    // this closure is looked up by that same type tag.
                    let d: &D = unsafe { ptr.cast::<D>().as_ref_unchecked() };
                    match D::fields(d) {
                        // TODO: the below is taking a newly allocated collection, unpacking it,
                        // and repacking it, only to hand it to a continuation that will unpack it
                        Ok(fields) => Ok(fields.into_iter().map(|(k, v)| (k.into(), v)).collect()),
                        Err(leaf) => Err(leaf.into()),
                    }
                },
                fields_of_node: |erased_node| {
                    let node: D::Node = erased_node.try_into().map_err(|erased| {
                        DualError::InvalidNode(AnyNode {
                            erased,
                            ty: TypeId::of::<D>(),
                        })
                    })?;
                    Ok(match D::fields_of_node(node) {
                        // TODO: the below is taking a newly allocated collection, unpacking it,
                        // and repacking it, only to hand it to a continuation that will unpack it
                        Ok(fields) => Ok(fields.into_iter().map(Into::into).collect()),
                        Err(leaf) => Err(leaf.into()),
                    })
                },
                leaf: |erased_leaf| {
                    let leaf: D::Leaf = erased_leaf.try_into().map_err(|erased| {
                        DualError::InvalidLeaf(AnyLeaf {
                            erased,
                            ty: TypeId::of::<D>(),
                        })
                    })?;
                    let node: D::Node = leaf.into();
                    Ok(node.into())
                },
                field: |erased_field| {
                    let field: D::Field = erased_field.try_into().map_err(|erased| {
                        DualError::InvalidField(AnyField {
                            erased,
                            parent_ty: TypeId::of::<D>(),
                        })
                    })?;
                    let branch: D::Branch = field.into();
                    Ok(branch.into())
                },
                field_type: |erased_field| {
                    let field: D::Field = erased_field.try_into().map_err(|erased| {
                        DualError::InvalidField(AnyField {
                            erased,
                            parent_ty: TypeId::of::<D>(),
                        })
                    })?;
                    Ok(D::field_type(field))
                },
                type_name: any::type_name::<D>(),
            },
        );
        let () = D::register_all_field_types(self);
    }

    /*
    #[inline]
    fn field(&self, &AnyField { erased, parent_ty }: &AnyField) -> Result<AnyNode, DualError> {
        let conversions = self
            .dispatch
            .get(&parent_ty)
            .ok_or(DualError::UnregisteredType(parent_ty))?;
        Ok(AnyNode {
            erased: (conversions.branch)((conversions.field)(erased)?)?,
            ty: (conversions.field_type)(erased)?,
        })
    }
    */
}

// TODO: delete and replace with `D::Field::any(&self)`
/// Erases a field and tags it with its canonical parent type.
#[inline]
fn any_field<D>(field: D::Field) -> AnyField
where
    D: Dual,
{
    AnyField {
        erased: field.into(),
        parent_ty: TypeId::of::<D::Deref>(),
    }
}

/// Registers `D` and every term type reachable through its fields.
///
/// Registering the same type more than once has no effect.
///
/// # Panics
///
/// Panics if the global [`REGISTRY`] lock is poisoned.
#[inline]
pub fn register<D>()
where
    D: Dual,
{
    #[expect(
        clippy::unwrap_used,
        reason = "a poisoned lock means another panic already occurred"
    )]
    let () = REGISTRY.write().unwrap().register::<D>();
}

/// Return the node stored at every rooted path in a term.
///
/// The root type and every field type reachable from it are registered before
/// traversal.
///
/// # Errors
///
/// Returns [`DualError`] if a type is unregistered, an erased value is invalid,
/// two nodes conflict at one path, or a [`Dual`] implementation produces an
/// invalid decomposition.
///
/// # Panics
///
/// Panics if the global [`REGISTRY`] lock is poisoned.
#[inline]
#[expect(
    clippy::unwrap_in_result,
    reason = "a poisoned lock means another panic already occurred"
)]
pub fn pin_up<D>(d: &D) -> Result<HashMap<Arc<RootedPath>, AnyNode>, DualError>
where
    D: Dual,
{
    let () = register::<D>();
    #[expect(
        clippy::unwrap_used,
        reason = "a poisoned lock means another panic already occurred"
    )]
    let registry = REGISTRY.read().unwrap();
    let mut pinned = HashMap::new();
    let () = pin_up_term(
        &AnyTerm::new(d),
        Arc::new(RootedPath::Root),
        &mut pinned,
        &registry,
    )?;
    Ok(pinned)
}

/// Rebuild a term from a map produced by [`pin_up`].
///
/// Extra map entries are ignored; every field demanded by the reconstructed
/// nodes must be present and well-typed.
///
/// # Errors
///
/// Returns [`DualError`] if the root or a required child is missing, a node has
/// the wrong type, an erased node is invalid, or reconstruction by [`Dual`]
/// fails.
#[inline]
pub fn from_pinned<D, S>(pinned: &StdHashMap<Arc<RootedPath>, AnyNode, S>) -> Result<D, DualError>
where
    D: Dual,
    S: BuildHasher,
{
    let () = register::<D>();
    let root = Arc::new(RootedPath::Root);
    let Some(any_node) = pinned.get(&root) else {
        return Err(DualError::MissingNode { at: root });
    };
    let node: D::Node = typed_node::<D>(any_node)?;
    D::from_node(node, PinnedFields { path: root, pinned })
}

/// Recursively pins `term` and each of its descendants by rooted path.
#[inline]
fn pin_up_term(
    term: &AnyTerm<'_>,
    path: Arc<RootedPath>,
    pinned: &mut HashMap<Arc<RootedPath>, AnyNode>,
    registry: &Registry,
) -> Result<(), DualError> {
    let node = match registry.fields(term)? {
        Err(leaf) => registry.leaf(&AnyLeaf {
            erased: leaf,
            ty: term.ty,
        })?,
        Ok(fields) => {
            #[expect(
                clippy::expect_used,
                reason = "`Dual::fields` returns `Err(leaf)` rather than `Ok({})` for leaves"
            )]
            let first_field = *fields
                .keys()
                .next()
                .expect("INTERNAL ERROR (`coterms`): branch with no fields");
            let conversions = registry
                .dispatch
                .get(&term.ty)
                .ok_or(DualError::UnregisteredType(term.ty))?;
            let node = AnyNode {
                erased: (conversions.branch)((conversions.field)(first_field)?)?,
                ty: term.ty,
            };
            let () = pin_node(pinned, Arc::clone(&path), node)?;
            #[expect(clippy::iter_over_hash_type, reason = "order doesn't matter")]
            for (field, value) in fields {
                let child = Arc::new(RootedPath::Step {
                    path: Arc::clone(&path),
                    field,
                });
                let () = pin_up_term(&value, child, pinned, registry)?;
            }
            return Ok(());
        }
    };
    pin_node(pinned, path, node)
}

/// Inserts a pinned node, rejecting a different node already at the same path.
#[inline]
fn pin_node(
    pinned: &mut HashMap<Arc<RootedPath>, AnyNode>,
    path: Arc<RootedPath>,
    node: AnyNode,
) -> Result<(), DualError> {
    match pinned.entry(Arc::clone(&path)) {
        hash_map::Entry::Vacant(vacant) => {
            let _: &mut AnyNode = vacant.insert(node);
            Ok(())
        }
        hash_map::Entry::Occupied(occupied) => Err(DualError::Conflict {
            at: path,
            existing: occupied.remove(),
            incoming: node,
        }),
    }
}

/// Validates a tagged node and converts it to `D`'s concrete node type.
#[inline]
fn typed_node<D>(node: &AnyNode) -> Result<D::Node, DualError>
where
    D: Dual,
{
    let ty = TypeId::of::<D::Deref>();
    if node.ty != ty {
        return Err(DualError::MistypedNode {
            actual: *node,
            expected: ty,
        });
    }
    node.erased
        .try_into()
        .map_err(|erased| DualError::InvalidNode(AnyNode { erased, ty }))
}

/// Check round-trip consistency for some type `D: Dual`
/// between `D::Node <-> ErasedNode` and `D::Field <-> ErasedField`.
#[cfg(test)]
macro_rules! check_dual {
    ($D:ty) => {
        #[::pbt::pbt]
        fn branch_node_usize_commutes(&branch: &<$D as $crate::Dual>::Branch) {
            #[expect(clippy::as_conversions, reason = "[don draper voice] that's what the tests are for!")]
            let branch_usize = branch as usize;
            let node: <$D as $crate::Dual>::Node = branch.into();
            #[expect(clippy::as_conversions, reason = "[don draper voice] that's what the tests are for!")]
            let node_usize = node as usize;
            assert_eq!(
                branch_usize, node_usize,
                "\r\n{branch:?} --> {branch_usize} (branch)\r\n|        !\r\nV        V\r\n{node:?} --> {node_usize} (node)",
            );
        }

        #[::pbt::pbt]
        fn term_coterm_term_roundtrip(term: &$D) {
            let () = $crate::register::<$D>();
            let coterm = $crate::Frontier::complete(term);
            let roundtrip: Result<$D, $crate::DualError> = coterm.dual();
            let expected = Ok(term.clone());
            assert_eq!(
                roundtrip, expected,
                "{term:?} -> {coterm:?} -> {roundtrip:?} =/= {expected:?}",
            );
        }

        #[::pbt::pbt]
        fn term_pinned_term_roundtrip(term: &$D) {
            let pinned = $crate::pin_up(term).expect("failed to pin up term");
            let roundtrip: Result<$D, $crate::DualError> = $crate::from_pinned(&pinned);
            let expected = Ok(term.clone());
            assert_eq!(
                roundtrip, expected,
                "{term:?} -> {pinned:?} -> {roundtrip:?} =/= {expected:?}",
            );
        }

        #[::pbt::pbt]
        fn eta_expansion_roundtrip(d: &$D) {
            match $crate::Dual::fields(d) {
                Ok(fields) => {
                    let first_field = *fields.keys().next().expect("no fields on an alleged non-leaf");
                    #[expect(unused_variables, reason = "TODO: what's up with this lint? looks incorrect, maybe macro hygiene?")]
                    let branch: <$D as $crate::Dual>::Branch = first_field.into();
                    for (&field, any_term) in &fields {
                        let field_ty = <$D as $crate::Dual>::field_type(field);
                        assert_eq!(field_ty, any_term.ty, "`field_type` assigned `{field:?} |-> {field_ty:?}`, but this field takes `{:?}`", any_term.ty);
                        let other_branch: <$D as $crate::Dual>::Branch = field.into();
                        assert_eq!(other_branch, branch, "fields {first_field:?} and {field:?} disagree on their branch");
                    }
                    let node: <$D as $crate::Dual>::Node = branch.into();
                    let keys: $crate::HashSet<<$D as $crate::Dual>::Field> = fields.keys().copied().collect();
                    let roundtrip: Result<$D, _> = <$D as $crate::Dual>::from_node(node, $crate::CloneFields { fields });
                    let expected = Ok(d);
                    assert_eq!(roundtrip.as_ref(), expected, "{d:?} -> {branch:?} (branch) -> {node:?} (node) -> {roundtrip:?} =/= {expected:?}");
                    let synthetic = <$D as $crate::Dual>::fields_of_node(node);
                    assert_eq!(
                        synthetic.as_ref(),
                        Ok(&keys),
                        "{d:?} --[Dual::fields]-> Ok({keys:?}) (keys of fields) --> {node:?} (node) --[Dual::fields_of_node]-> {synthetic:?} =/= Ok({keys:?})",
                    );
                }
                Err(leaf) => {
                    let node: <$D as $crate::Dual>::Node = leaf.into();
                    let roundtrip: Result<$D, _> = <$D as $crate::Dual>::from_node(node, $crate::NoFields);
                    let expected = Ok(d);
                    assert_eq!(roundtrip.as_ref(), expected, "{d:?} -> {leaf:?} (leaf) -> {node:?} (node) -> {roundtrip:?} =/= {expected:?}");
                    let synthetic = <$D as $crate::Dual>::fields_of_node(node);
                    assert_eq!(synthetic, Err(leaf), "{d:?} --[Dual::fields]-> Err({leaf:?}) (leaf) --> {node:?} (node) --[Dual::fields_of_node]-> {synthetic:?} =/= Err({leaf:?})");
                }
            }
        }

        #[::pbt::pbt]
        fn leaf_node_usize_commutes(&leaf: &<$D as $crate::Dual>::Leaf) {
            #[expect(clippy::as_conversions, reason = "[don draper voice] that's what the tests are for!")]
            let leaf_usize = leaf as usize;
            let node: <$D as $crate::Dual>::Node = leaf.into();
            #[expect(clippy::as_conversions, reason = "[don draper voice] that's what the tests are for!")]
            let node_usize = node as usize;
            assert_eq!(
                leaf_usize, node_usize,
                "\r\n{leaf:?} --> {leaf_usize} (leaf)\r\n|        !\r\nV        V\r\n{node:?} --> {node_usize} (node)",
            );
        }

        #[::pbt::pbt]
        fn leaf_usize_leaf_roundtrip(&leaf: &<$D as $crate::Dual>::Leaf) {
            let tmp: $crate::ErasedLeaf = leaf.into();
            let roundtrip: Result<<$D as $crate::Dual>::Leaf, _> = tmp.try_into();
            let expected = Ok(leaf);
            assert_eq!(
                roundtrip, expected,
                "{leaf:?} -> {tmp:?} -> {roundtrip:?} =/= {expected:?}",
            );
        }

        #[::pbt::pbt]
        fn node_usize_node_roundtrip(&node: &<$D as $crate::Dual>::Node) {
            let tmp: $crate::ErasedNode = node.into();
            let roundtrip: Result<<$D as $crate::Dual>::Node, _> = tmp.try_into();
            let expected = Ok(node);
            assert_eq!(
                roundtrip, expected,
                "{node:?} -> {tmp:?} -> {roundtrip:?} =/= {expected:?}",
            );
        }

        #[::pbt::pbt]
        fn field_usize_field_roundtrip(&field: &<$D as $crate::Dual>::Field) {
            let tmp: $crate::ErasedField = field.into();
            let roundtrip: Result<<$D as $crate::Dual>::Field, _> = tmp.try_into();
            let expected = Ok(field);
            assert_eq!(
                roundtrip, expected,
                "{field:?} -> {tmp:?} -> {roundtrip:?} =/= {expected:?}",
            );
        }

        #[::pbt::pbt]
        fn unique_valid_frontier_per_term(leaves: &$crate::HashMap<::alloc::sync::Arc<$crate::RootedPath>, $crate::ErasedLeaf>) {
            let coterm = $crate::Frontier::<$D> {
                _phantom: core::marker::PhantomData,
                holes: <$crate::HashMap<_, _> as $crate::HashMapExt>::new(),
                leaves: leaves.clone(),
            };
            let Ok(term) = coterm.dual() else {
                return;
            };
            let roundtrip = $crate::Frontier::complete(&term);
            assert_eq!(roundtrip, coterm, "{coterm:?} -> {term:?} -> {roundtrip:?} =/= {coterm:?}");
        }

        #[::pbt::pbt]
        fn usize_leaf_usize_roundtrip(&usize: &$crate::ErasedLeaf) {
            let Ok(tmp): Result<<$D as $crate::Dual>::Leaf, _> = usize.try_into() else {
                return;
            };
            let roundtrip: $crate::ErasedLeaf = tmp.into();
            assert_eq!(
                roundtrip, usize,
                "{usize:?} -> {tmp:?} -> {roundtrip:?} =/= {usize:?}",
            );
        }

        #[::pbt::pbt]
        fn usize_node_usize_roundtrip(&usize: &$crate::ErasedNode) {
            let Ok(tmp): Result<<$D as $crate::Dual>::Node, _> = usize.try_into() else {
                return;
            };
            let roundtrip: $crate::ErasedNode = tmp.into();
            assert_eq!(
                roundtrip, usize,
                "{usize:?} -> {tmp:?} -> {roundtrip:?} =/= {usize:?}",
            );
        }

        #[::pbt::pbt]
        fn usize_field_usize_roundtrip(&usize: &$crate::ErasedField) {
            let Ok(tmp): Result<<$D as $crate::Dual>::Field, _> = usize.try_into() else {
                return;
            };
            let roundtrip: $crate::ErasedField = tmp.into();
            assert_eq!(
                roundtrip, usize,
                "{usize:?} -> {tmp:?} -> {roundtrip:?} =/= {usize:?}",
            );
        }

        #[::pbt::pbt]
        fn term_hole_filling_roundtrip(d: &$D, seed: &u64) {
            let () = $crate::register::<$D>();
            let mut prng = pbt::WyRand::new(*seed);
            let mut frontier = $crate::Frontier::<$D>::new();

            let mut work = vec![(
                $crate::RootedHole {
                    path: ::alloc::sync::Arc::new($crate::RootedPath::Root),
                    ty: ::core::any::TypeId::of::<<$D as $crate::Dual>::Deref>(),
                },
                $crate::AnyTerm::new(d),
            )];
            while let Some(n) = core::num::NonZero::new(work.len()) {
                #[expect(
                    clippy::as_conversions,
                    clippy::cast_possible_truncation,
                    reason = "bounded by hardware"
                )]
                let i: usize = prng.rand() as usize % n;
                let (hole, any_term) = work.swap_remove(i);
                let (node, holes) = {
                    let registry = $crate::REGISTRY.read().unwrap();
                    let conversions = registry.dispatch.get(&hole.ty).unwrap_or_else(|| panic!("unregistered type: {:?}", hole.ty));
                    match (conversions.fields)(any_term.erased) {
                        Err(leaf) => {
                            let node: $crate::ErasedNode = (conversions.leaf)(leaf).unwrap();
                            (node, vec![])
                        }
                        Ok(fields) => {
                            let first_field: $crate::ErasedField = *fields.keys().next().expect("no fields on an alleged non-leaf");
                            let branch: $crate::ErasedBranch = (conversions.field)(first_field).unwrap();
                            let node: $crate::ErasedNode = (conversions.branch)(branch).unwrap();
                            let holes: Vec<($crate::RootedHole, $crate::AnyTerm)> = fields.into_iter().map(|(field, fill)| (
                                $crate::RootedHole {
                                    path: ::alloc::sync::Arc::new($crate::RootedPath::Step {
                                        path: ::alloc::sync::Arc::clone(&hole.path),
                                        field,
                                    }),
                                    ty: (conversions.field_type)(field).unwrap(),
                                },
                                fill,
                            )).collect();
                            (node, holes)
                        }
                    }
                };
                let _: $crate::HashSet<$crate::ErasedField> = frontier.fill(&hole.path, node).unwrap();
                let () = work.extend(holes);
            }

            let roundtrip = frontier.dual();
            assert_eq!(roundtrip.as_ref(), Ok(d), "{d:?} -> ...hole-filling... -> {roundtrip:?} =/= Ok({d:?})");
        }
    };
}

#[cfg(test)]
use check_dual;

#[cfg(test)]
mod tests {
    use super::{Frontier, from_pinned, pin_up};

    #[derive(Clone, Debug, Eq, PartialEq, crate::Dual)]
    struct Pair {
        left: bool,
        right: bool,
    }

    #[test]
    fn complete_round_trip_reuses_the_parent_branch() {
        let pair = Pair {
            left: false,
            right: true,
        };

        assert_eq!(Frontier::complete(&pair).dual(), Ok(pair));
    }

    #[test]
    fn pinned_round_trip_reconstructs_typed_fields() {
        let pair = Pair {
            left: false,
            right: true,
        };
        let round_trip = pin_up(&pair).and_then(|pinned| from_pinned(&pinned));

        assert_eq!(round_trip, Ok(pair));
    }
}
