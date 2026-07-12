use {
    crate::{Dual, DualError, ErasedNode, ErasedSlot, Frontier, RootedPath},
    ahash::HashSet,
    alloc::sync::Arc,
    core::iter,
};

pub trait Redex {
    type InputPath;
    type OutputPath;

    fn recv(
        &mut self,
        hole: &Self::InputPath,
        node: ErasedNode,
    ) -> Result<impl Iterator<Item = (Self::OutputPath, ErasedNode)>, DualError>;
}

impl<D> Redex for Frontier<D>
where
    D: Dual,
{
    type InputPath = Arc<RootedPath>;
    type OutputPath = Arc<RootedPath>;

    #[inline]
    fn recv(
        &mut self,
        hole: &Self::InputPath,
        node: ErasedNode,
    ) -> Result<impl Iterator<Item = (Self::OutputPath, ErasedNode)>, DualError> {
        let _new_holes: HashSet<ErasedSlot> = self.fill(hole, node)?;
        Ok(iter::once((Arc::clone(hole), node)))
    }
}
