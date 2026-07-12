#[cofunction]
fn is_finite<T>(slice: &[T]) -> bool {
    match *slice {
        [] => true,
        [_head, ref tail @ ..] => is_finite(tail),
    }
}
