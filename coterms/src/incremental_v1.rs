// TODO
// Incremental computations start with a `Pending(Root)` and, when updated,
// check to see if the node they're waiting for (initially `Root`)
// is available; if so, they process that node and may emit more `Pending`s.
// Once no `Pending`s remain, the value is finished computing (somehow).
//
// Note that this can be run *both* for full terms *and* for co-terms,
// even (especially!) partial co-terms still under construction.
//
// Also, `Pending` is suspicously similar to `holes`;
// can we literally use `Frontier` with some kind of computation type?
// Actually, yes, I think `Pending` is a hole in the *output* data type,
// and it waits for a place in the *input* data type. This is beautiful...
//
// This should also naturally translate to Rust's `async` runtime,
// but that might be a bit complex and we should save it for later.
// On the other hand, `poll` might be exactly the right abstraction here.

// TODO: Start by incrementalizing the height of a binary tree.
// Then, once that works, see how easily we can implement short-circuiting
// for "depth of the shallowest node," which doesn't need to traverse the whole tree.
// Then, finally, whole-hog is *incremental bidirectional typechecking*.

use {
    crate::{
        AnyNode, AnySlot, Dual as _, DualError, ErasedNode, ErasedSlot, ErasedTerm, Frontier,
        RootedHole, RootedPath, any_slot, peano::Peano, typed_node,
    },
    ahash::{HashMap, HashMapExt as _, HashSet, HashSetExt as _},
    alloc::sync::Arc,
    core::{
        any::{Any, TypeId},
        iter,
        marker::PhantomData,
        ptr,
    },
    std::collections::hash_map,
};

// TODO: start anew by overhauling untyped logic; instead, store untyped/tagged spines
// but have a generic root field (continuation/hook) and store *typed* holes,
// ideally with some dyn-comptable method of enumerating type-safe slots therein.
// then, associate each hole with a `Future` (even if we don't use `async`);
// for normal "user-provided" nodes, this is just "check again if it's been filled,"
// but for computations this can trigger a cascade.

/*
struct Continuations {
    cases: HashMap<Arc<RootedPath>, Box<dyn FnOnce(ErasedNode) -> Incremental>>,
}

struct LocalContinuations {
    cases: HashMap<AnySlot, Box<dyn FnOnce(ErasedNode) -> Incremental>>,
}

enum Incremental {
    Complete(Box<dyn Any>),
    Continuation(LocalContinuations),
}

struct WacoKid<X, Y>
where
    X: Dual,
{
    continuations: HashMap<Arc<RootedPath>, Continuations>,
    /// "If I get a new node at this path, which existing paths will I update?"
    dependents: HashMap<Arc<RootedPath>, HashSet<Arc<RootedPath>>>,
    input: Frontier<X>, // TODO: oh my god we can fucking ELIDE THE INPUT
    _phantmo: PhantomData<Y>,
}
*/

/*
impl<Y> DependencyGraph<Y> {
    #[inline]
    fn new<X, Continuation>(continuation: Continuation) -> Self
    where
        Continuation: 'static + FnOnce(X::Node) -> Incremental,
        X: Dual,
    {
        let root = Arc::new(RootedPath::Root);
        Self {
            _phantom: PhantomData,
            continuations: iter::once((
                Arc::clone(&root),
                AwaitingInputs {
                    not_yet_received: iter::once(Arc::clone(&root)).collect(),
                    continuation: Box::new(move |mut received| {
                        let erased: ErasedNode = received.remove(&root).expect(
                            "INTERNAL ERROR (`coterms`): incremental computation missing its root",
                        );
                        assert!(
                            received.is_empty(),
                            "INTERNAL ERROR (`coterms`): incremental root expects multiple inputs",
                        );
                        let node: X::Node = typed_node::<X>(&AnyNode {
                            erased,
                            ty: TypeId::of::<X>(),
                        })
                        .expect("INTERNAL ERROR (`coterms`): incremental root mistyped");
                        continuation(node)
                    }),
                    received: HashMap::new(),
                },
            ))
            .collect(),
        }
    }

    #[inline]
    fn receive(&mut self, path: Arc<RootedPath>, node: ErasedNode) {
        let hash_map::Entry::Occupied(entry) = self.continuations.entry(path) else {
            panic!("INTERNAL ERROR (`coterms`): missing incremental update destination");
        };
    }
}
*/

/*
impl<X, Y> WacoKid<X, Y>
where
    X: Dual,
{
    #[inline]
    #[expect(
        clippy::unwrap_in_result,
        reason = "a poisoned lock means another panic already occurred"
    )]
    fn fill(&mut self, hole: &Arc<RootedPath>, node: ErasedNode) -> Result<Option<Y>, DualError> {
        let new_slots = self.input.fill(hole, node)?;
        // TODO
        let Some(dependents) = self.dependents.remove(hole) else {
            return Ok(None);
        };
        for dependent in dependents {
            // TODO
        }
        todo!()
    }

    #[inline]
    fn new<Continuation>(continuation: Continuation) -> Self
    where
        Continuation: 'static + FnOnce(X::Node) -> Incremental,
    {
        Self {
            input: Frontier::<X>::new(),
            // TODO
        }
    }
}
*/

enum Continuation {
    Insert {
        node: ErasedNode,
        // TODO: dedicated `enum`s for the children of each node for the domain of this function
        watch: HashMap<AnySlot, Box<dyn FnOnce(ErasedNode) -> Continuation>>,
    },
    Update(Box<dyn FnOnce(ErasedNode) -> Continuation>),
}

struct Height {
    output: Frontier<Peano>,
    // TODO: We could have multiple "reactions" all waiting to modify the same output node.
    // Is this acceptable and/or the user's responsibility to prevent?
    // If not, how in the world do we somehow synchronize/mutex this?
    reactions: HashMap<InputPath, HashMap<OutputPath, Box<dyn FnOnce(ErasedNode) -> Continuation>>>,
}

#[repr(transparent)]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct InputPath(Arc<RootedPath>);

#[repr(transparent)]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct OutputPath(Arc<RootedPath>);

impl InputPath {
    #[inline]
    fn wrap(path: &Arc<RootedPath>) -> &Self {
        unsafe {
            ptr::from_ref::<Arc<RootedPath>>(path)
                .cast::<Self>()
                .as_ref_unchecked()
        }
    }
}

impl OutputPath {
    #[inline]
    fn wrap(path: &Arc<RootedPath>) -> &Self {
        unsafe {
            ptr::from_ref::<Arc<RootedPath>>(path)
                .cast::<Self>()
                .as_ref_unchecked()
        }
    }
}

impl Height {
    #[inline]
    fn fill(
        &mut self,
        input_hole: &InputPath,
        input_node: ErasedNode,
    ) -> Result<Option<Peano>, DualError> {
        let Some(reactions) = self.reactions.remove(input_hole) else {
            return Ok(None);
        };
        for (output_path, f) in reactions {
            match f(input_node) {
                Continuation::Insert {
                    node: output_node,
                    watch,
                } => {
                    let _: HashSet<ErasedSlot> = self.output.fill(&output_path.0, output_node)?;
                    for (slot, g) in watch {
                        let dup: Option<_> = self.reactions.insert(
                            InputPath(Arc::new(RootedPath::Step {
                                path: Arc::clone(&input_hole.0),
                                slot: slot.erased,
                            })),
                            g,
                        );
                        assert!(dup.is_none());
                    }
                }
                Continuation::Update(g) => {
                    let dup: Option<_> = self.reactions.insert(input_hole, g);
                    assert!(dup.is_none());
                }
            }
        }
        todo!()
    }

    #[inline]
    fn new(reaction: Box<dyn FnOnce(ErasedNode) -> Continuation>) -> Self {
        Self {
            output: Frontier::new(),
            reactions: iter::once((
                InputPath(Arc::new(RootedPath::Root)),
                iter::once((OutputPath(Arc::new(RootedPath::Root)), reaction)).collect(),
            ))
            .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        clippy::panic,
        clippy::unwrap_used,
        reason = "failing tests ought to panic"
    )]

    use {
        super::*,
        crate::{AnyTerm, ErasedBranch, REGISTRY, binary_tree::BinaryTree, peano::Peano, register},
        alloc::sync::Arc,
        core::num::NonZero,
        pbt::pbt,
    };

    #[inline]
    fn height_of_full_tree(tree: &BinaryTree) -> Peano {
        match *tree {
            BinaryTree::Leaf => Peano::Zero,
            BinaryTree::Branch { ref lhs, ref rhs } => Peano::Successor(Box::new(max(
                &height_of_full_tree(lhs),
                &height_of_full_tree(rhs),
            ))),
        }
    }

    #[inline]
    fn shallowest_of_full_tree(tree: &BinaryTree) -> Peano {
        match *tree {
            BinaryTree::Leaf => Peano::Zero,
            BinaryTree::Branch { ref lhs, ref rhs } => Peano::Successor(Box::new(min(
                &shallowest_of_full_tree(lhs),
                &shallowest_of_full_tree(rhs),
            ))),
        }
    }

    #[inline]
    fn max(lhs: &Peano, rhs: &Peano) -> Peano {
        match *lhs {
            Peano::Zero => rhs.clone(),
            Peano::Successor(ref lhs_pred) => match *rhs {
                Peano::Zero => lhs.clone(),
                Peano::Successor(ref rhs_pred) => {
                    Peano::Successor(Box::new(max(lhs_pred, rhs_pred)))
                }
            },
        }
    }

    #[inline]
    fn min(lhs: &Peano, rhs: &Peano) -> Peano {
        match *lhs {
            Peano::Zero => Peano::Zero,
            Peano::Successor(ref lhs_pred) => match *rhs {
                Peano::Zero => Peano::Zero,
                Peano::Successor(ref rhs_pred) => {
                    Peano::Successor(Box::new(min(lhs_pred, rhs_pred)))
                }
            },
        }
    }

    #[test]
    fn height_of_root_is_zero() {
        assert_eq!(height_of_full_tree(&BinaryTree::Leaf), Peano::Zero);
    }

    #[test]
    fn height_of_branch_is_one() {
        assert_eq!(
            height_of_full_tree(&BinaryTree::Branch {
                lhs: Arc::new(BinaryTree::Leaf),
                rhs: Arc::new(BinaryTree::Leaf),
            }),
            Peano::Successor(Box::new(Peano::Zero)),
        );
    }

    #[test]
    fn height_of_left_leg_is_two() {
        assert_eq!(
            height_of_full_tree(&BinaryTree::Branch {
                lhs: Arc::new(BinaryTree::Branch {
                    lhs: Arc::new(BinaryTree::Leaf),
                    rhs: Arc::new(BinaryTree::Leaf),
                }),
                rhs: Arc::new(BinaryTree::Leaf),
            }),
            Peano::Successor(Box::new(Peano::Successor(Box::new(Peano::Zero)))),
        );
    }

    #[test]
    fn height_of_right_leg_is_two() {
        assert_eq!(
            height_of_full_tree(&BinaryTree::Branch {
                lhs: Arc::new(BinaryTree::Leaf),
                rhs: Arc::new(BinaryTree::Branch {
                    lhs: Arc::new(BinaryTree::Leaf),
                    rhs: Arc::new(BinaryTree::Leaf),
                }),
            }),
            Peano::Successor(Box::new(Peano::Successor(Box::new(Peano::Zero)))),
        );
    }

    #[test]
    fn shallowest_of_root_is_zero() {
        assert_eq!(shallowest_of_full_tree(&BinaryTree::Leaf), Peano::Zero);
    }

    #[test]
    fn shallowest_of_branch_is_one() {
        assert_eq!(
            shallowest_of_full_tree(&BinaryTree::Branch {
                lhs: Arc::new(BinaryTree::Leaf),
                rhs: Arc::new(BinaryTree::Leaf),
            }),
            Peano::Successor(Box::new(Peano::Zero)),
        );
    }

    #[test]
    fn shallowest_of_left_leg_is_one() {
        assert_eq!(
            shallowest_of_full_tree(&BinaryTree::Branch {
                lhs: Arc::new(BinaryTree::Branch {
                    lhs: Arc::new(BinaryTree::Leaf),
                    rhs: Arc::new(BinaryTree::Leaf),
                }),
                rhs: Arc::new(BinaryTree::Leaf),
            }),
            Peano::Successor(Box::new(Peano::Zero)),
        );
    }

    #[test]
    fn shallowest_of_right_leg_is_one() {
        assert_eq!(
            shallowest_of_full_tree(&BinaryTree::Branch {
                lhs: Arc::new(BinaryTree::Leaf),
                rhs: Arc::new(BinaryTree::Branch {
                    lhs: Arc::new(BinaryTree::Leaf),
                    rhs: Arc::new(BinaryTree::Leaf),
                }),
            }),
            Peano::Successor(Box::new(Peano::Zero)),
        );
    }

    #[pbt]
    fn incremental_height_matches_total(tree: &BinaryTree, seed: &u64) {
        let () = register::<BinaryTree>();
        let expected = height_of_full_tree(tree);

        let tree_frontier = Frontier::complete(tree);
        let mut incremental = Height::new();
        let mut actual = None;

        let mut prng = pbt::WyRand::new(*seed);
        let mut work = vec![(
            RootedHole {
                path: Arc::new(RootedPath::Root),
                ty: TypeId::of::<BinaryTree>(),
            },
            AnyTerm::new(tree),
        )];
        'incremental: while let Some(n) = NonZero::new(work.len()) {
            #[expect(
                clippy::as_conversions,
                clippy::cast_possible_truncation,
                reason = "bounded by hardware"
            )]
            let i: usize = prng.rand() as usize % n;
            let (hole, any_term) = work.swap_remove(i);
            let (node, holes) = {
                let registry = REGISTRY.read().unwrap();
                let dispatch = registry
                    .dispatch
                    .get(&hole.ty)
                    .unwrap_or_else(|| panic!("unregistered type: {:?}", hole.ty));
                match (dispatch.fields)(any_term.erased) {
                    Err(leaf) => {
                        let node: ErasedNode = (dispatch.leaf)(leaf).unwrap();
                        (node, vec![])
                    }
                    Ok(fields) => {
                        let first_slot: ErasedSlot = *fields
                            .keys()
                            .next()
                            .expect("no fields on an alleged non-leaf");
                        let branch: ErasedBranch = (dispatch.slot)(first_slot).unwrap();
                        let node: ErasedNode = (dispatch.branch)(branch).unwrap();
                        let holes: Vec<(RootedHole, AnyTerm)> = fields
                            .into_iter()
                            .map(|(slot, fill)| {
                                (
                                    RootedHole {
                                        path: Arc::new(RootedPath::Step {
                                            path: Arc::clone(&hole.path),
                                            slot,
                                        }),
                                        ty: (dispatch.slot_type)(slot).unwrap(),
                                    },
                                    fill,
                                )
                            })
                            .collect();
                        (node, holes)
                    }
                }
            };
            if let Some(peano) = incremental.fill(&hole.path, node).unwrap() {
                actual = Some(peano);
                break 'incremental;
            }
            let () = work.extend(holes);
        }

        assert_eq!(actual, Some(expected));
    }
}
