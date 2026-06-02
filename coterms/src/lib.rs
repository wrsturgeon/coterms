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

use {
    ahash::{HashMap, HashMapExt as _, HashSet},
    alloc::sync::Arc,
    core::{any::TypeId, fmt},
    pbt::Pbt,
    std::collections::hash_map,
};

#[derive(Debug, Eq, PartialEq)]
enum DualError {
    Conflict(Arc<RootedPath>, AnyNode, AnyNode),
    Incomplete(HashSet<RootedHole>),
    InvalidLeaf(AnyLeaf),
    InvalidNode(AnyNode),
    InvalidSlot(AnySlot),
    MissingNode(Arc<RootedPath>),
    MistypedLeaf(AnyLeaf, TypeId),
    MistypedNode(AnyNode, TypeId),
    MistypedSlot(AnySlot, TypeId),
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
struct ErasedLeaf(usize);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
struct ErasedNode(usize);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
struct ErasedSlot(usize);

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

#[derive(Debug, Eq, PartialEq)]
struct Frontier {
    holes: HashSet<RootedHole>,
    leaves: HashSet<RootedLeaf>,
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
    Root {
        ty: TypeId,
    },
    Step {
        path: Arc<Self>,
        /// The particular slot into which this value is inserted.
        slot: AnySlot,
    },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RootedHole {
    path: Arc<RootedPath>,
    ty: TypeId,
}

trait Dual: 'static + Sized {
    type Leaf: Copy
        + fmt::Debug
        + Into<ErasedLeaf>
        + TryFrom<ErasedLeaf, Error = ErasedLeaf>
        + Into<Self::Node>;
    type Node: Copy + fmt::Debug + Into<ErasedNode> + TryFrom<ErasedNode, Error = ErasedNode>;
    type Slot: Copy
        + fmt::Debug
        + Into<ErasedSlot>
        + TryFrom<ErasedSlot, Error = ErasedSlot>
        + Into<Self::Node>;
    fn fields(node: Self::Node) -> Result<HashSet<AnySlot>, Self::Leaf>;
    // TODO: The below shouldn't need a `HashMap` at all if we hash-cons internal structure!
    fn from_nodes(
        nodes: &HashMap<Arc<RootedPath>, AnyNode>,
        path: Arc<RootedPath>,
    ) -> Result<Self, DualError>;
    fn to_leaves(&self, leaves: &mut HashSet<RootedLeaf>, path: Arc<RootedPath>);
}

impl Frontier {
    // TODO: use `AsRef` or `Borrow` to allow skipping `ty: TypeId` fields
    // so we can run PBT on frontiers without having to generate `TypeId`s
    #[inline]
    fn dual<D>(&self) -> Result<D, DualError>
    where
        D: Dual,
    {
        if !self.holes.is_empty() {
            return Err(DualError::Incomplete(self.holes.clone()));
        }
        let mut nodes: HashMap<Arc<RootedPath>, AnyNode> = HashMap::new();
        #[expect(clippy::iter_over_hash_type, reason = "Order doesn't matter.")]
        for leaf in &self.leaves {
            // TODO: CRUCIAL: this might not be a `D`!
            // We need a global `fn(AnySlot) -> AnyNode`.
            let leaf_node: AnyNode = any_node::<D>(typed_leaf::<D>(&leaf.leaf)?.into());
            let () = match nodes.entry(Arc::clone(&leaf.path)) {
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
                // TODO: CRUCIAL: this might not be a `D`!
                // We need a global map from `TypeId` to `fn(ErasedSlot) -> ErasedNode`.
                let node: AnyNode = any_node::<D>(typed_slot::<D>(slot)?.into());
                visitor = Arc::clone(path);
                let () = match nodes.entry(Arc::clone(&visitor)) {
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
        D::from_nodes(
            &nodes,
            Arc::new(RootedPath::Root {
                ty: TypeId::of::<D>(),
            }),
        )
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

fn root<D>() -> Arc<RootedPath>
where
    D: Dual,
{
    Arc::new(RootedPath::Root {
        ty: TypeId::of::<D>(),
    })
}

fn root_hole<D>() -> RootedHole
where
    D: Dual,
{
    RootedHole {
        path: root::<D>(),
        ty: TypeId::of::<D>(),
    }
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
        fn node_fields_node_roundtrip(&node: &<$D as Dual>::Node) {
            match <$D as Dual>::fields(node) {
                Ok(slots) => {
                    for any_slot in slots {
                        // TODO: CRUCIAL: this might not be a `D`!
                        // We need a global `fn(AnySlot) -> AnyNode`.
                        let slot = $crate::typed_slot::<$D>(&any_slot).unwrap();
                        let roundtrip: <$D as Dual>::Node = slot.into();
                        assert_eq!(
                            node, roundtrip,
                            "{node:?} -> {slot:?} -> {roundtrip:?} =/= {node:?}",
                        )
                    }
                }
                Err(leaf) => {
                    let roundtrip: <$D as Dual>::Node = leaf.into();
                    assert_eq!(
                        node, roundtrip,
                        "{node:?} -> {leaf:?} -> {roundtrip:?} =/= {node:?}",
                    )
                }
            }
        }

        #[::pbt::pbt]
        fn leaf_node_leaf_roundtrip(&leaf: &<$D as Dual>::Leaf) {
            let node: <$D as Dual>::Node = leaf.into();
            let roundtrip = <$D as Dual>::fields(node);
            assert_eq!(
                roundtrip,
                Err(leaf),
                "{leaf:?} -> {node:?} -> {roundtrip:?} =/= {leaf:?}",
            )
        }

        #[::pbt::pbt]
        fn slot_node_slot_roundtrip(&slot: &<$D as Dual>::Slot) {
            let node: <$D as Dual>::Node = slot.into();
            let roundtrip = <$D as Dual>::fields(node);
            let any_slot = $crate::any_slot::<$D>(slot);
            assert!(
                roundtrip.as_ref().is_ok_and(|set| set.contains(&any_slot)),
                "{slot:?} -> {node:?} -> {roundtrip:?} doesn't contain {any_slot:?}",
            )
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
