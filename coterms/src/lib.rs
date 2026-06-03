//! Top-down node-by-node construction and incremental computing.

#![expect(
    clippy::arbitrary_source_item_ordering,
    clippy::doc_paragraphs_missing_punctuation,
    clippy::missing_docs_in_private_items,
    missing_docs,
    warnings,
    reason = "TODO"
)]

extern crate alloc;

mod binary_tree;
mod incremental;
mod option;

use {
    ahash::{HashMap, HashMapExt as _, HashSet, HashSetExt as _, RandomState},
    alloc::sync::Arc,
    core::{
        any::{Any, TypeId},
        fmt,
        hash::Hash,
        marker::PhantomData,
        ptr,
    },
    pbt::Pbt,
    std::{collections::hash_map, sync::RwLock},
};

static REGISTRY: RwLock<Registry> = RwLock::new(Registry {
    #[expect(clippy::unusual_byte_groupings, reason = "readability")]
    dispatch: HashMap::with_hasher(RandomState::with_seeds(
        0xBAAD_5EED_BAAD_C0DE,
        0xC0DE_CAFE_DECAF_BAD,
        0xDEFEC8ED__BAAD_D00D,
        0x1337_1337_1337_1337,
    )),
});

// TODO: remove unused variants
#[derive(Debug, Eq, PartialEq)]
enum DualError {
    Conflict(Arc<RootedPath>, AnyNode, AnyNode),
    Incomplete(HashSet<RootedHole>),
    InvalidBranch(AnyBranch),
    InvalidLeaf(AnyLeaf),
    InvalidNode(AnyNode),
    InvalidSlot(AnySlot),
    MissingNode(Arc<RootedPath>),
    MissingSlot(AnySlot),
    MistypedField {
        actual_type: TypeId,
        expected_type: TypeId,
        slot: AnySlot,
    },
    MistypedLeaf(AnyLeaf, TypeId),
    MistypedNode(AnyNode, TypeId),
    MistypedRoot {
        decoded_as: TypeId,
        encoded_as: TypeId,
    },
    MistypedSlot(AnySlot, TypeId),
    UnregisteredType(TypeId),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct AnyBranch {
    index: ErasedBranch,
    ty: TypeId,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct AnyLeaf {
    index: ErasedLeaf,
    ty: TypeId,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct AnyNode {
    index: ErasedNode,
    ty: TypeId,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct AnySlot {
    index: ErasedSlot,
    ty: TypeId,
}

#[derive(Debug)]
struct AnyTerm<'term> {
    lifetime: PhantomData<&'term ()>,
    ptr: *const (),
    ty: TypeId,
}

/// For testing only. Implements `Fields` by cloning pre-existing fields.
#[cfg(test)]
struct CloneFields<'field, D>
where
    D: Dual,
{
    fields: HashMap<D::Slot, AnyTerm<'field>>,
}

struct ErasedConversions {
    branch: fn(ErasedBranch) -> Result<ErasedNode, DualError>,
    fields: for<'term> fn(
        *const (),
        PhantomData<&'term ()>,
    ) -> Result<HashMap<ErasedSlot, AnyTerm<'term>>, ErasedLeaf>,
    leaf: fn(ErasedLeaf) -> Result<ErasedNode, DualError>,
    slot: fn(ErasedSlot) -> Result<ErasedNode, DualError>,
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
struct ErasedBranch(usize);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
struct ErasedLeaf(usize);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
struct ErasedNode(usize);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
struct ErasedSlot(usize);

#[derive(Debug, Eq, PartialEq)]
struct Frontier {
    holes: HashSet<RootedHole>,
    leaves: HashSet<RootedLeaf>,
    ty: TypeId,
}

struct LazyFields<'pinup> {
    path: Arc<RootedPath>,
    pinup: &'pinup HashMap<Arc<RootedPath>, AnyNode>,
}

/// For testing only. Implements `Fields` but panics on `.field(..)`.
#[cfg(test)]
struct NoFields;

struct Registry {
    dispatch: HashMap<TypeId, ErasedConversions>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RootedHole {
    path: Arc<RootedPath>,
    ty: TypeId,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RootedLeaf {
    leaf: AnyLeaf,
    path: Arc<RootedPath>,
}

/// A type-erased path from some node
/// all the way up to the global root.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum RootedPath {
    Root,
    Step {
        path: Arc<Self>,
        /// The particular slot into which this value is inserted.
        slot: AnySlot,
    },
}

trait Dual: 'static + Clone {
    type Branch: Copy
        + fmt::Debug
        + Into<ErasedBranch>
        + TryFrom<ErasedBranch, Error = ErasedBranch>
        + Into<Self::Node>;
    type Leaf: Copy
        + fmt::Debug
        + Into<ErasedLeaf>
        + TryFrom<ErasedLeaf, Error = ErasedLeaf>
        + Into<Self::Node>;
    type Node: Copy + fmt::Debug + Into<ErasedNode> + TryFrom<ErasedNode, Error = ErasedNode>;
    type Slot: Copy
        + fmt::Debug
        + Eq
        + Hash
        + Into<ErasedSlot>
        + TryFrom<ErasedSlot, Error = ErasedSlot>
        + Into<Self::Branch>;
    fn fields(&self) -> Result<HashMap<Self::Slot, AnyTerm<'_>>, Self::Leaf>;
    // TODO: Instead of passing a `HashMap`, just hand this an already-unerased `Self::Node`
    // and have it generate a *local* continuation (via `Slot`s instead of `Path`s).
    // Then, fields of any type -- which are already referenced --
    // can be erased as `(*const _, TypeId)` and dispatched behind the scenes.
    fn from_node<F>(node: Self::Node, fields: F) -> Result<Self, DualError>
    where
        F: Fields<Self>;
    fn register_all_field_types();
    // TODO: is the below still necessary?
    /// The type with which this slot should be filled.
    fn slot_type(slot: Self::Slot) -> TypeId;
}

trait Fields<D>
where
    D: Dual,
{
    fn field<T>(&self, slot: D::Slot) -> Result<T, DualError>
    where
        T: Dual;
}

#[cfg(test)]
impl<D> Fields<D> for CloneFields<'_, D>
where
    D: Dual,
{
    #[inline]
    fn field<T>(&self, slot: D::Slot) -> Result<T, DualError>
    where
        T: Dual,
    {
        let any_term: &AnyTerm<'_> = self
            .fields
            .get(&slot)
            .ok_or_else(|| DualError::MissingSlot(any_slot::<D>(slot)))?;
        let ty = TypeId::of::<T>();
        if any_term.ty != ty {
            return Err(DualError::MistypedSlot(any_slot::<D>(slot), any_term.ty));
        }
        // SAFETY: Assuming `AnyTerm` has been soundly constructed,
        // this reinterpretation is valid b/c `ty == ty` above.
        let t: &T = unsafe { any_term.ptr.cast::<T>().as_ref_unchecked() };
        Ok(t.clone())
    }
}

impl<'term> AnyTerm<'term> {
    #[inline]
    fn new<D>(field: &'term D) -> Self
    where
        D: Dual,
    {
        Self {
            lifetime: PhantomData,
            ptr: ptr::from_ref(field).cast(),
            ty: TypeId::of::<D>(),
        }
    }
}

impl Frontier {
    #[inline]
    pub fn complete<D>(d: &D) -> Self
    where
        D: Dual,
    {
        let mut leaves = HashSet::new();
        let () = register::<D>();
        #[expect(
            clippy::unwrap_used,
            reason = "a poisoned lock means another panic already occurred"
        )]
        let registry = REGISTRY.read().unwrap();
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
            holes: HashSet::new(),
            leaves,
            ty: TypeId::of::<D>(),
        }
    }

    #[inline]
    fn complete_leaves(
        term: &AnyTerm<'_>,
        path: Arc<RootedPath>,
        leaves: &mut HashSet<RootedLeaf>,
        registry: &Registry,
    ) -> Result<(), DualError> {
        let () = match registry.fields(term)? {
            Err(leaf) => {
                let _: bool = leaves.insert(RootedLeaf {
                    leaf: AnyLeaf {
                        index: leaf,
                        ty: term.ty,
                    },
                    path,
                });
            }
            Ok(fields) =>
            {
                #[expect(clippy::iter_over_hash_type, reason = "order doesn't matter")]
                for (slot, field) in fields {
                    let extended_path = Arc::new(RootedPath::Step {
                        path: Arc::clone(&path),
                        slot: AnySlot {
                            index: slot,
                            ty: term.ty,
                        },
                    });
                    let () = Self::complete_leaves(&field, extended_path, leaves, registry)?;
                }
            }
        };
        Ok(())
    }

    // TODO: use `AsRef` or `Borrow` to allow skipping `ty: TypeId` fields
    // so we can run PBT on frontiers without having to generate `TypeId`s
    #[inline]
    #[expect(
        clippy::unwrap_in_result,
        reason = "a poisoned lock means another panic already occurred"
    )]
    pub fn dual<D>(&self) -> Result<D, DualError>
    where
        D: Dual,
    {
        let ty = TypeId::of::<D>();
        if ty != self.ty {
            return Err(DualError::MistypedRoot {
                decoded_as: ty,
                encoded_as: self.ty,
            });
        }
        let () = register::<D>();
        #[expect(
            clippy::unwrap_used,
            reason = "a poisoned lock means another panic already occurred"
        )]
        let registry = REGISTRY.read().unwrap();
        if !self.holes.is_empty() {
            return Err(DualError::Incomplete(self.holes.clone()));
        }
        // TODO: This `HashMap` shouldn't be necessary at all if we hash-cons internal structure!
        let mut pinup: HashMap<Arc<RootedPath>, AnyNode> = HashMap::new();
        #[expect(clippy::iter_over_hash_type, reason = "order doesn't matter")]
        for leaf in &self.leaves {
            let leaf_node: AnyNode = registry.leaf(&leaf.leaf)?;
            let () = match pinup.entry(Arc::clone(&leaf.path)) {
                hash_map::Entry::Vacant(vacant) => {
                    let _: &mut AnyNode = vacant.insert(leaf_node);
                }
                hash_map::Entry::Occupied(occupied) => {
                    if *occupied.get() != leaf_node {
                        return Err(DualError::Conflict(
                            Arc::clone(&leaf.path),
                            occupied.get().clone(),
                            leaf_node,
                        ));
                    }
                }
            };
            let mut visitor = Arc::clone(&leaf.path);
            'walk_up: while let RootedPath::Step { ref path, ref slot } = *visitor {
                let node: AnyNode = registry.slot(slot)?;
                visitor = Arc::clone(path);
                let () = match pinup.entry(Arc::clone(&visitor)) {
                    hash_map::Entry::Vacant(vacant) => {
                        let _: &mut AnyNode = vacant.insert(node);
                    }
                    hash_map::Entry::Occupied(occupied) => {
                        if *occupied.get() != node {
                            return Err(DualError::Conflict(
                                Arc::clone(&visitor),
                                occupied.get().clone(),
                                node,
                            ));
                        }
                    }
                };
            }
        }
        let root_path = Arc::new(RootedPath::Root);
        let Some(root_any_node) = pinup.get(&root_path) else {
            return Err(DualError::MissingNode(root_path));
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
}

impl<D> Fields<D> for LazyFields<'_>
where
    D: Dual,
{
    #[inline]
    fn field<T>(&self, slot: D::Slot) -> Result<T, DualError>
    where
        T: Dual,
    {
        let path = Arc::new(RootedPath::Step {
            path: Arc::clone(&self.path),
            slot: any_slot::<D>(slot),
        });
        let Some(any_node) = self.pinup.get(&path) else {
            return Err(DualError::MissingNode(path));
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
    fn field<T>(&self, slot: D::Slot) -> Result<T, DualError>
    where
        T: Dual,
    {
        panic!("Called `.field(..)` on `NoFields`")
    }
}

impl Registry {
    #[inline]
    fn fields<'term>(
        &self,
        &AnyTerm { lifetime, ptr, ty }: &AnyTerm<'term>,
    ) -> Result<Result<HashMap<ErasedSlot, AnyTerm<'term>>, ErasedLeaf>, DualError> {
        let f = self
            .dispatch
            .get(&ty)
            .ok_or(DualError::UnregisteredType(ty))?
            .fields;
        Ok(f(ptr, lifetime))
    }

    #[inline]
    fn leaf(&self, &AnyLeaf { index, ty }: &AnyLeaf) -> Result<AnyNode, DualError> {
        let f = self
            .dispatch
            .get(&ty)
            .ok_or(DualError::UnregisteredType(ty))?
            .leaf;
        Ok(AnyNode {
            index: f(index)?,
            ty,
        })
    }

    #[inline]
    fn register<D>(&mut self)
    where
        D: Dual,
    {
        let ty = TypeId::of::<D>();
        let _: Option<_> = self.dispatch.insert(
            ty,
            ErasedConversions {
                branch: |erased_branch| {
                    let branch: D::Branch = erased_branch.try_into().map_err(|index| {
                        DualError::InvalidBranch(AnyBranch {
                            index,
                            ty: TypeId::of::<D>(),
                        })
                    })?;
                    let node: D::Node = branch.into();
                    Ok(node.into())
                },
                fields: |ptr: *const (), _lifetime: PhantomData<&'_ ()>| {
                    // SAFETY: Invariant. Extremely dangerous.
                    let d: &D = unsafe { ptr.cast::<D>().as_ref_unchecked() };
                    match D::fields(d) {
                        // TODO: the below is taking a newly allocated collection, unpacking it,
                        // and repacking it, only to hand it to a continuation that will unpack it
                        Ok(fields) => Ok(fields.into_iter().map(|(k, v)| (k.into(), v)).collect()),
                        Err(leaf) => Err(leaf.into()),
                    }
                },
                leaf: |erased_leaf| {
                    let leaf: D::Leaf = erased_leaf.try_into().map_err(|index| {
                        DualError::InvalidLeaf(AnyLeaf {
                            index,
                            ty: TypeId::of::<D>(),
                        })
                    })?;
                    let node: D::Node = leaf.into();
                    Ok(node.into())
                },
                slot: |erased_slot| {
                    let slot: D::Slot = erased_slot.try_into().map_err(|index| {
                        DualError::InvalidSlot(AnySlot {
                            index,
                            ty: TypeId::of::<D>(),
                        })
                    })?;
                    let branch: D::Branch = slot.into();
                    let node: D::Node = branch.into();
                    Ok(node.into())
                },
            },
        );
    }

    #[inline]
    fn slot(&self, &AnySlot { index, ty }: &AnySlot) -> Result<AnyNode, DualError> {
        let f = self
            .dispatch
            .get(&ty)
            .ok_or(DualError::UnregisteredType(ty))?
            .slot;
        Ok(AnyNode {
            index: f(index)?,
            ty,
        })
    }
}

#[inline]
fn any_leaf<D>(leaf: D::Leaf) -> AnyLeaf
where
    D: Dual,
{
    AnyLeaf {
        index: leaf.into(),
        ty: TypeId::of::<D>(),
    }
}

#[inline]
fn any_node<D>(node: D::Node) -> AnyNode
where
    D: Dual,
{
    AnyNode {
        index: node.into(),
        ty: TypeId::of::<D>(),
    }
}

#[inline]
fn any_slot<D>(slot: D::Slot) -> AnySlot
where
    D: Dual,
{
    AnySlot {
        index: slot.into(),
        ty: TypeId::of::<D>(),
    }
}

#[inline]
fn register<D>()
where
    D: Dual,
{
    // TODO: Is it worth holding an `&mut` handle while recursing?
    // This function is only called ~once, so it's truly not a performance concern.
    #[expect(
        clippy::unwrap_used,
        reason = "a poisoned lock means another panic already occurred"
    )]
    if REGISTRY
        .read()
        .unwrap()
        .dispatch
        .contains_key(&TypeId::of::<D>())
    {
        return;
    }
    #[expect(
        clippy::unwrap_used,
        reason = "a poisoned lock means another panic already occurred"
    )]
    let () = REGISTRY.write().unwrap().register::<D>();
    let () = D::register_all_field_types();
}

#[inline]
fn typed_leaf<D>(leaf: &AnyLeaf) -> Result<D::Leaf, DualError>
where
    D: Dual,
{
    let ty = TypeId::of::<D>();
    if leaf.ty != ty {
        return Err(DualError::MistypedLeaf(leaf.clone(), ty));
    }
    leaf.index
        .try_into()
        .map_err(|index| DualError::InvalidLeaf(AnyLeaf { index, ty }))
}

#[inline]
fn typed_node<D>(node: &AnyNode) -> Result<D::Node, DualError>
where
    D: Dual,
{
    let ty = TypeId::of::<D>();
    if node.ty != ty {
        return Err(DualError::MistypedNode(node.clone(), ty));
    }
    node.index
        .try_into()
        .map_err(|index| DualError::InvalidNode(AnyNode { index, ty }))
}

#[inline]
fn typed_slot<D>(slot: &AnySlot) -> Result<D::Slot, DualError>
where
    D: Dual,
{
    let ty = TypeId::of::<D>();
    if slot.ty != ty {
        return Err(DualError::MistypedSlot(slot.clone(), ty));
    }
    slot.index
        .try_into()
        .map_err(|index| DualError::InvalidSlot(AnySlot { index, ty }))
}

/// Check round-trip consistency for some type `D: Dual`
/// between `D::Node <-> ErasedNode` and `D::Slot <-> ErasedSlot`.
macro_rules! check_dual_roundtrip {
    ($D:ty) => {
        #[::pbt::pbt]
        fn branch_node_usize_commutes(&branch: &<$D as Dual>::Branch) {
            #[expect(clippy::as_conversions, reason = "[don draper voice] that's what the tests are for!")]
            let branch_usize = branch as usize;
            let node: <$D as Dual>::Node = branch.into();
            #[expect(clippy::as_conversions, reason = "[don draper voice] that's what the tests are for!")]
            let node_usize = node as usize;
            assert_eq!(
                branch_usize, node_usize,
                "\r\n{branch:?} --> {branch_usize} (branch)\r\n|        !\r\nV        V\r\n{node:?} --> {node_usize} (node)",
            );
        }

        #[::pbt::pbt]
        fn leaf_node_usize_commutes(&leaf: &<$D as Dual>::Leaf) {
            #[expect(clippy::as_conversions, reason = "[don draper voice] that's what the tests are for!")]
            let leaf_usize = leaf as usize;
            let node: <$D as Dual>::Node = leaf.into();
            #[expect(clippy::as_conversions, reason = "[don draper voice] that's what the tests are for!")]
            let node_usize = node as usize;
            assert_eq!(
                leaf_usize, node_usize,
                "\r\n{leaf:?} --> {leaf_usize} (leaf)\r\n|        !\r\nV        V\r\n{node:?} --> {node_usize} (node)",
            );
        }

        #[::pbt::pbt]
        fn eta_expansion_roundtrip(d: &$D) {
            match d.fields() {
                Ok(fields) => {
                    let (&first_slot, _) = fields.iter().next().expect("no fields on an alleged non-leaf");
                    let branch: <$D as Dual>::Branch = first_slot.into();
                    for (&slot, any_term) in &fields {
                        let slot_ty = <$D as Dual>::slot_type(slot);
                        assert_eq!(slot_ty, any_term.ty, "`slot_type` assigned `{slot:?} |-> {slot_ty:?}`, but this slot takes `{:?}`", any_term.ty);
                        let other_branch: <$D as Dual>::Branch = branch.into();
                        assert_eq!(other_branch, branch, "slots {first_slot:?} and {slot:?} disagree on their branch");
                    }
                    let node: <$D as Dual>::Node = branch.into();
                    let roundtrip: Result<$D, _> = <$D as Dual>::from_node(node, $crate::CloneFields { fields });
                    let expected = Ok(d);
                    assert_eq!(roundtrip.as_ref(), expected, "{d:?} -> {branch:?} (branch) -> {node:?} (node) -> {roundtrip:?} =/= {expected:?}");
                }
                Err(leaf) => {
                    let node: <$D as Dual>::Node = leaf.into();
                    let roundtrip: Result<$D, _> = <$D as Dual>::from_node(node, $crate::NoFields);
                    let expected = Ok(d);
                    assert_eq!(roundtrip.as_ref(), expected, "{d:?} -> {leaf:?} (leaf) -> {node:?} (node) -> {roundtrip:?} =/= {expected:?}");
                }
            }
        }

        #[::pbt::pbt]
        fn leaf_usize_leaf_roundtrip(&leaf: &<$D as Dual>::Leaf) {
            let tmp: ErasedLeaf = leaf.into();
            let roundtrip: Result<<$D as Dual>::Leaf, _> = tmp.try_into();
            let expected = Ok(leaf);
            assert_eq!(
                roundtrip, expected,
                "{leaf:?} -> {tmp:?} -> {roundtrip:?} =/= {expected:?}",
            );
        }

        #[::pbt::pbt]
        fn node_usize_node_roundtrip(&node: &<$D as Dual>::Node) {
            let tmp: ErasedNode = node.into();
            let roundtrip: Result<<$D as Dual>::Node, _> = tmp.try_into();
            let expected = Ok(node);
            assert_eq!(
                roundtrip, expected,
                "{node:?} -> {tmp:?} -> {roundtrip:?} =/= {expected:?}",
            );
        }

        #[::pbt::pbt]
        fn slot_usize_slot_roundtrip(&slot: &<$D as Dual>::Slot) {
            let tmp: ErasedSlot = slot.into();
            let roundtrip: Result<<$D as Dual>::Slot, _> = tmp.try_into();
            let expected = Ok(slot);
            assert_eq!(
                roundtrip, expected,
                "{slot:?} -> {tmp:?} -> {roundtrip:?} =/= {expected:?}",
            );
        }

        #[::pbt::pbt]
        fn usize_leaf_usize_roundtrip(&usize: &ErasedLeaf) {
            let Ok(tmp): Result<<$D as Dual>::Leaf, _> = usize.try_into() else {
                return;
            };
            let roundtrip: ErasedLeaf = tmp.into();
            assert_eq!(
                roundtrip, usize,
                "{usize:?} -> {tmp:?} -> {roundtrip:?} =/= {usize:?}",
            );
        }

        #[::pbt::pbt]
        fn usize_node_usize_roundtrip(&usize: &ErasedNode) {
            let Ok(tmp): Result<<$D as Dual>::Node, _> = usize.try_into() else {
                return;
            };
            let roundtrip: ErasedNode = tmp.into();
            assert_eq!(
                roundtrip, usize,
                "{usize:?} -> {tmp:?} -> {roundtrip:?} =/= {usize:?}",
            );
        }

        #[::pbt::pbt]
        fn usize_slot_usize_roundtrip(&usize: &ErasedSlot) {
            let Ok(tmp): Result<<$D as Dual>::Slot, _> = usize.try_into() else {
                return;
            };
            let roundtrip: ErasedSlot = tmp.into();
            assert_eq!(
                roundtrip, usize,
                "{usize:?} -> {tmp:?} -> {roundtrip:?} =/= {usize:?}",
            );
        }
    };
}

use check_dual_roundtrip;
