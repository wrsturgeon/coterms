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
    Conflict(Arc<Place>, ErasedNode, ErasedNode),
    Incomplete,
    InvalidNode(ErasedNode),
    InvalidSlot(ErasedSlot),
    MissingNode(Arc<Place>),
}

// TODO: reinstate `Leaf`, `ErasedLeaf`, and `leaf_node(Leaf) -> Node`
// so we can statically guarantee that leaves on the frontier are really leaves.
// then add a `fn(Node) -> Either<Leaf, Internal>` and round-trip PBT it

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
struct ErasedNode(usize);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Pbt)]
struct ErasedSlot(usize);

#[derive(Debug, Eq, PartialEq)]
struct Frontier {
    holes: HashSet<Arc<Place>>,
    leaves: HashSet<Filled<ErasedNode>>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Filled<Fill> {
    fill: Fill,
    slot: Arc<Place>,
}

/// A type-erased path from some node
/// all the way up to the global root.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Path {
    Root,
    Step(Filled<ErasedSlot>),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Place {
    path: Path,
    ty: TypeId,
}

trait Dual: 'static + Sized {
    type Node: Copy + fmt::Debug + Into<ErasedNode> + TryFrom<ErasedNode, Error = ErasedNode>;
    type Slot: Copy + fmt::Debug + Into<ErasedSlot> + TryFrom<ErasedSlot, Error = ErasedSlot>;
    // TODO: The below shouldn't need a `HashMap` at all if we hash-cons internal structure!
    fn from_nodes(nodes: &HashMap<Arc<Place>, ErasedNode>, path: Path) -> Result<Self, DualError>;
    fn to_nodes(&self, leaves: &mut HashSet<Filled<ErasedNode>>, path: Path);
    fn node(slot: Self::Slot) -> Self::Node;
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
            return Err(DualError::Incomplete);
        }
        let mut nodes: HashMap<Arc<Place>, ErasedNode> = HashMap::new();
        #[expect(clippy::iter_over_hash_type, reason = "Order doesn't matter.")]
        for leaf in &self.leaves {
            let () = match nodes.entry(Arc::clone(&leaf.slot)) {
                hash_map::Entry::Vacant(vacant) => {
                    let _: &mut ErasedNode = vacant.insert(leaf.fill);
                }
                hash_map::Entry::Occupied(occupied) => {
                    if *occupied.get() != leaf.fill {
                        return Err(DualError::Conflict(
                            Arc::clone(&leaf.slot),
                            *occupied.get(),
                            leaf.fill,
                        ));
                    }
                }
            };
            let mut visitor = Arc::clone(&leaf.slot);
            'walk_up: while let Path::Step(ref filled) = visitor.path {
                let node: ErasedNode =
                // TODO: CRUCIAL: this might not be a `D`!
                // We need a global map from `TypeId` to `fn(ErasedSlot) -> ErasedNode`.
                    D::node(filled.fill.try_into().map_err(DualError::InvalidSlot)?).into();
                visitor = Arc::clone(&filled.slot);
                let () = match nodes.entry(Arc::clone(&visitor)) {
                    hash_map::Entry::Vacant(vacant) => {
                        let _: &mut ErasedNode = vacant.insert(node);
                    }
                    hash_map::Entry::Occupied(occupied) => {
                        if *occupied.get() != node {
                            return Err(DualError::Conflict(
                                Arc::clone(&visitor),
                                *occupied.get(),
                                node,
                            ));
                        }
                    }
                };
            }
        }
        D::from_nodes(&nodes, Path::Root)
    }
}

fn root<D>() -> Arc<Place>
where
    D: Dual,
{
    Arc::new(Place {
        path: Path::Root,
        ty: TypeId::of::<D>(),
    })
}

/// Check round-trip consistency for some type `D: Dual`
/// between `D::Node <-> ErasedNode` and `D::Slot <-> ErasedSlot`.
macro_rules! check_dual_roundtrip {
    ($D:ty) => {
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
