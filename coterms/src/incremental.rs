use crate::{DualError, ErasedNode, redex::Redex};

pub struct Min;

impl Redex for Min {
    type InputPath = Arc<RootedPath>;
    type OutputPath = Arc<RootedPath>;

    fn recv(
        &mut self,
        hole: &Self::InputPath,
        node: ErasedNode,
    ) -> Result<impl Iterator<Item = (Self::OutputPath, ErasedNode)>, DualError> {
        asdf
    }
}
