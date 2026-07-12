#![allow(
    clippy::missing_inline_in_public_items,
    reason = "All public items are macros."
)]

//! Macros for the `coterms` crate.

use proc_macro::TokenStream;

/// Top-down node-by-node construction of a term.
#[proc_macro_derive(Dual)]
pub fn coterm(item: TokenStream) -> TokenStream {
    coterms_macro2::coterm(item.into()).into()
}
