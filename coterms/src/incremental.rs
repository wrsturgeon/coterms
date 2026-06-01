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
