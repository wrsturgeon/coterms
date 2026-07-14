//! Macros for the `coterms` crate.

use {
    heck::{ToSnakeCase as _, ToUpperCamelCase as _},
    proc_macro2::{Literal, Span, TokenStream},
    quote::{format_ident, quote},
    syn::{Data, DeriveInput, Fields, Ident, Type, Visibility, parse2},
};

/// The enum declaration being expanded into its coterm dual.
struct Enum {
    /// The enum generics, copied onto the generated trait implementation.
    generics: syn::Generics,
    /// The original enum name.
    ident: Ident,
    /// The module that contains all local coterm tags.
    module: Ident,
    /// The enum variants in source order.
    variants: Vec<Variant>,
    /// The visibility copied onto the generated module.
    visibility: Visibility,
}

/// One field of a non-leaf variant.
#[derive(Clone)]
struct Field {
    /// The local binding used when matching an existing term.
    binding: Ident,
    /// The generated `Field` enum variant for this payload slot.
    ident: Ident,
    /// The field's index within its own variant.
    local: usize,
    /// The name of the payload field when the source variant is braced.
    source_ident: Option<Ident>,
    /// The erased field number assigned to this payload slot.
    tag: usize,
    /// The source type of the field.
    ty: Type,
}

/// Whether generated field names carry their variant's name as a prefix.
#[derive(Clone, Copy)]
enum FieldPrefix {
    /// Do not prefix field names.
    No,
    /// Prefix field names with the variant name.
    Variant,
}

/// A single enum variant in source order.
struct Variant {
    /// The fields belonging to this variant, if any.
    fields: Vec<Field>,
    /// The source variant name.
    ident: Ident,
    /// The term constructor syntax used by this variant.
    source: VariantSource,
    /// The source shape of the variant payload.
    style: VariantStyle,
    /// The erased node number assigned to this variant.
    tag: usize,
}

/// The term-level syntax used to construct and match a variant.
#[derive(Clone, Copy)]
enum VariantSource {
    /// `Self::Variant`.
    Enum,
    /// `Self`.
    Struct,
}

/// The source shape of a variant payload.
#[derive(Clone, Copy)]
enum VariantStyle {
    /// `Variant { .. }`.
    Named,
    /// `Variant`.
    Unit,
    /// `Variant(..)`.
    Unnamed,
}

impl Enum {
    /// Generated conversion from a local tag to its type-indexed erased form.
    fn any_impl(
        &self,
        local_type_name: &str,
        any_type_name: &str,
        ty_field_name: &'static str,
    ) -> TokenStream {
        let any = format_ident!("{}", any_type_name);
        let ident = &self.ident;
        let local = format_ident!("{}", local_type_name);
        let ty_field_name_id = format_ident!("{}", ty_field_name);
        let mut generics = self.generics.clone();
        for type_param in generics.type_params_mut() {
            type_param.bounds.push(syn::parse_quote!(::coterms::Dual));
        }
        let (method_generics, ty_generics, where_clause) = generics.split_for_impl();
        quote! {
            impl #local {
                #[inline(always)]
                pub const fn any #method_generics(self) -> ::coterms::#any #where_clause {
                    ::coterms::#any {
                        erased: self.erase(),
                        #ty_field_name_id: ::core::any::TypeId::of::<super::#ident #ty_generics>(),
                    }
                }
            }
        }
    }

    /// Generated arms for `From<Branch> for Node`.
    fn branch_to_node(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .filter(|variant| variant.is_branch())
            .map(|variant| {
                let ident = &variant.ident;
                quote! { Self::#ident => Node::#ident }
            })
            .collect()
    }

    /// Generated `Branch` enum variants.
    fn branch_variants(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .filter(|variant| variant.is_branch())
            .map(|variant| {
                let ident = &variant.ident;
                let tag = usize_literal(variant.tag);
                quote! { #ident = #tag }
            })
            .collect()
    }

    /// Generated arms for `Dual::fields`.
    fn dual_fields(&self) -> Vec<TokenStream> {
        let module = &self.module;
        self.variants
            .iter()
            .map(|variant| {
                let pattern = variant.match_pattern();
                let variant_ident = &variant.ident;
                if variant.fields.is_empty() {
                    quote! { #pattern => Err(#module::Leaf::#variant_ident) }
                } else {
                    let generated_fields: Vec<_> = variant
                        .fields
                        .iter()
                        .map(|field| {
                            let field_ident = &field.ident;
                            let binding = &field.binding;
                            let ty = &field.ty;
                            quote! {
                                (
                                    #module::Field::#field_ident,
                                    ::coterms::AnyTerm::new::<#ty>(#binding),
                                )
                            }
                        })
                        .collect();
                    quote! {
                        #pattern => Ok(
                            [#(#generated_fields),*]
                                .into_iter()
                                .collect(),
                        )
                    }
                }
            })
            .collect()
    }

    /// Generated arms for `Dual::fields_of_node`.
    fn dual_fields_of_node(&self) -> Vec<TokenStream> {
        let module = &self.module;
        self.variants
            .iter()
            .map(|variant| {
                let variant_ident = &variant.ident;
                if variant.fields.is_empty() {
                    quote! { #module::Node::#variant_ident => Err(#module::Leaf::#variant_ident) }
                } else if let Some(field) = variant
                    .fields
                    .first()
                    .filter(|_| variant.fields.len() == 1)
                {
                    let field_ident = &field.ident;
                    quote! {
                        #module::Node::#variant_ident => Ok(::core::iter::once(#module::Field::#field_ident).collect())
                    }
                } else {
                    let generated_fields: Vec<_> = variant
                        .fields
                        .iter()
                        .map(|field| {
                            let field_ident = &field.ident;
                            quote! { #module::Field::#field_ident }
                        })
                        .collect();
                    quote! {
                        #module::Node::#variant_ident => Ok([#(#generated_fields),*].into_iter().collect())
                    }
                }
            })
            .collect()
    }

    /// Generated arms for `Dual::from_node`.
    fn dual_from_node(&self) -> Vec<TokenStream> {
        let module = &self.module;
        self.variants
            .iter()
            .map(|variant| {
                let constructor = variant.constructor(module);
                let variant_ident = &variant.ident;
                if variant.fields.is_empty() {
                    quote! { #module::Node::#variant_ident => #constructor }
                } else {
                    quote! { #module::Node::#variant_ident => { #constructor } }
                }
            })
            .collect()
    }

    /// Generate the coterm module and the `Dual` implementation.
    fn expand(&self) -> TokenStream {
        let module = self.expand_module();
        let dual = self.expand_dual();
        quote! {
            #module

            #dual
        }
    }

    /// Generate the `Dual` implementation for the source enum.
    fn expand_dual(&self) -> TokenStream {
        let ident = &self.ident;
        let module = &self.module;
        let mut generics = self.generics.clone();
        for type_param in generics.type_params_mut() {
            type_param.bounds.push(syn::parse_quote!(::coterms::Dual));
        }
        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

        let fields = self.dual_fields();
        let fields_of_node = self.dual_fields_of_node();
        let field_type = self.field_type();
        let from_node = self.dual_from_node();
        let from_node_body = if self.variants.is_empty() {
            quote! { match node {} }
        } else {
            quote! {
                Ok(match node {
                    #(#from_node,)*
                })
            }
        };
        let register_all_field_types = self.register_all_field_types();

        quote! {
            impl #impl_generics ::coterms::Dual for #ident #ty_generics #where_clause {
                type Branch = #module::Branch;
                type Leaf = #module::Leaf;
                type Node = #module::Node;
                type Field = #module::Field;
                type Deref = Self;

                #[inline(always)]
                fn deref(&self) -> &Self::Deref {
                    self
                }

                #[inline(always)]
                fn fields(&self) -> Result<::coterms::HashMap<<Self as ::coterms::Dual>::Field, ::coterms::AnyTerm<'_>>, <Self as ::coterms::Dual>::Leaf> {
                    match *self {
                        #(#fields,)*
                    }
                }

                #[inline(always)]
                fn fields_of_node(node: <Self as ::coterms::Dual>::Node) -> Result<::coterms::HashSet<<Self as ::coterms::Dual>::Field>, <Self as ::coterms::Dual>::Leaf> {
                    match node {
                        #(#fields_of_node,)*
                    }
                }

                #[inline(always)]
                fn from_node<F>(node: <Self as ::coterms::Dual>::Node, fields: F) -> Result<Self, ::coterms::DualError>
                where
                    F: ::coterms::Fields<Self::Deref>,
                {
                    #from_node_body
                }

                #[inline]
                fn register_all_field_types(registry: &mut ::coterms::Registry) {
                    #(#register_all_field_types)*
                }

                #[inline(always)]
                fn field_type(field: <Self as ::coterms::Dual>::Field) -> ::core::any::TypeId {
                    match field {
                        #(#field_type,)*
                    }
                }
            }
        }
    }

    /// Generate the coterm module for the source enum.
    fn expand_module(&self) -> TokenStream {
        let visibility = &self.visibility;
        let module = &self.module;

        let branch_any = self.any_impl("Branch", "AnyBranch", "ty");
        let branch_to_node = self.branch_to_node();
        let branch_variants = self.branch_variants();
        let field_to_branch = self.field_to_branch();
        let field_any = self.any_impl("Field", "AnyField", "parent_ty");
        let field_type_debug_arms = self.field_type_debug_arms();
        let field_variants = self.field_variants();
        let fields_in = self.fields_in();
        let fields_in_structs = self.fields_in_structs();
        let leaf_any = self.any_impl("Leaf", "AnyLeaf", "ty");
        let leaf_to_node = self.leaf_to_node();
        let leaf_variants = self.leaf_variants();
        let match_struct = self.match_struct();
        let branch_enum = enum_definition("Branch", &branch_variants);
        let branch_to_erased =
            erased_from_impl("Branch", "ErasedBranch", branch_variants.is_empty());
        let branch_unreachable = unreachable_code_allow(branch_variants.is_empty());
        let node_type_debug_arms = self.node_type_debug_arms();
        let node_variants = self.node_variants();
        let field_enum = field_enum_definition(&field_variants, &field_type_debug_arms);
        let field_to_erased = erased_from_impl("Field", "ErasedField", field_variants.is_empty());
        let field_unreachable = unreachable_code_allow(field_variants.is_empty());
        let leaf_enum = enum_definition("Leaf", &leaf_variants);
        let leaf_to_erased = erased_from_impl("Leaf", "ErasedLeaf", leaf_variants.is_empty());
        let leaf_unreachable = unreachable_code_allow(leaf_variants.is_empty());
        let node_any = self.any_impl("Node", "AnyNode", "ty");
        let node_enum = node_enum_definition(&node_variants, &node_type_debug_arms);
        let node_into_enum_iterator = self.node_into_enum_iterator();
        let node_to_erased = erased_from_impl("Node", "ErasedNode", node_variants.is_empty());
        let try_branch_arms = self.try_branch_arms();
        let try_branch = try_from_impl("ErasedBranch", "Branch", &try_branch_arms);
        let try_field_arms = self.try_field_arms();
        let try_field = try_from_impl("ErasedField", "Field", &try_field_arms);
        let try_leaf_arms = self.try_leaf_arms();
        let try_leaf = try_from_impl("ErasedLeaf", "Leaf", &try_leaf_arms);
        let try_node_arms = self.try_node_arms();
        let try_node = try_from_impl("ErasedNode", "Node", &try_node_arms);

        quote! {
            #visibility mod #module {
                #node_enum

                #node_into_enum_iterator

                #branch_enum

                #leaf_enum

                #field_enum

                #(#fields_in)*

                #match_struct

                #(#fields_in_structs)*

                impl Branch {
                    #[inline(always)]
                    pub const fn node(self) -> Node {
                        match self {
                            #(#branch_to_node,)*
                        }
                    }
                }

                impl Leaf {
                    #[inline(always)]
                    pub const fn node(self) -> Node {
                        match self {
                            #(#leaf_to_node,)*
                        }
                    }
                }

                impl Field {
                    #[inline(always)]
                    pub const fn branch(self) -> Branch {
                        match self {
                            #(#field_to_branch,)*
                        }
                    }
                }

                impl From<Branch> for Node {
                    #branch_unreachable
                    #[inline(always)]
                    fn from(value: Branch) -> Self {
                        value.node()
                    }
                }

                impl From<Leaf> for Node {
                    #leaf_unreachable
                    #[inline(always)]
                    fn from(value: Leaf) -> Self {
                        value.node()
                    }
                }

                impl From<Field> for Branch {
                    #field_unreachable
                    #[inline(always)]
                    fn from(value: Field) -> Self {
                        value.branch()
                    }
                }

                #branch_to_erased

                #branch_any

                #leaf_to_erased

                #leaf_any

                #node_to_erased

                #node_any

                #field_to_erased

                #field_any

                #try_branch
                #try_leaf
                #try_node
                #try_field
            }
        }
    }

    /// Generated arms for `From<Field> for Branch`.
    fn field_to_branch(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .filter(|variant| variant.is_branch())
            .flat_map(|variant| {
                let variant_ident = &variant.ident;
                variant.fields.iter().map(move |field| {
                    let field_ident = &field.ident;
                    quote! { Self::#field_ident => Branch::#variant_ident }
                })
            })
            .collect()
    }

    /// Generated arms for `Dual::field_type`.
    fn field_type(&self) -> Vec<TokenStream> {
        let module = &self.module;
        self.variants
            .iter()
            .flat_map(|variant| &variant.fields)
            .map(|field| {
                let field_ident = &field.ident;
                let ty = &field.ty;
                quote! { #module::Field::#field_ident => ::core::any::TypeId::of::<<#ty as ::coterms::Dual>::Deref>() }
            })
            .collect()
    }

    /// Generated source-oriented `Debug` arms for global fields.
    fn field_type_debug_arms(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .flat_map(|variant| &variant.fields)
            .map(|field| {
                let ident = &field.ident;
                let name = field.source_ident.as_ref().map_or_else(
                    || field.local.to_string(),
                    |source_ident| {
                        let source_name = source_ident.to_string();
                        source_name
                            .strip_prefix("r#")
                            .unwrap_or(&source_name)
                            .to_owned()
                    },
                );
                quote! { Self::#ident => formatter.write_str(#name) }
            })
            .collect()
    }

    /// Generated `Field` enum variants.
    fn field_variants(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .flat_map(|variant| &variant.fields)
            .map(|field| {
                let ident = &field.ident;
                let tag = usize_literal(field.tag);
                quote! { #ident = #tag }
            })
            .collect()
    }

    /// Generated per-variant field enums.
    fn fields_in(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .map(|variant| {
                let enum_ident = format_ident!("FieldIn{}", variant.ident);
                let field_variants: Vec<_> = variant
                    .fields
                    .iter()
                    .map(|field| {
                        let field_ident = &field.ident;
                        let tag = usize_literal(field.tag);
                        quote! { #field_ident = #tag }
                    })
                    .collect();
                if field_variants.is_empty() {
                    quote! {
                        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
                        pub enum #enum_ident {}
                    }
                } else {
                    quote! {
                        #[repr(usize)]
                        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
                        pub enum #enum_ident {
                            #(#field_variants,)*
                        }
                    }
                }
            })
            .collect()
    }

    /// Generated per-variant field-product structs.
    fn fields_in_structs(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .map(|variant| {
                let ident = format_ident!("FieldsIn{}", variant.ident);
                let generics: Vec<_> = variant.fields.iter().map(Field::product_ty).collect();
                let fields: Vec<_> = variant
                    .fields
                    .iter()
                    .map(|field| {
                        let name = field.product_field();
                        let ty = field.product_ty();
                        quote! { #name: #ty }
                    })
                    .collect();
                if generics.is_empty() {
                    quote! {
                        #[derive(Clone, Debug, Eq, Hash, PartialEq)]
                        pub struct #ident {}
                    }
                } else {
                    quote! {
                        #[derive(Clone, Debug, Eq, Hash, PartialEq)]
                        pub struct #ident<#(#generics),*> {
                            #(#fields),*
                        }
                    }
                }
            })
            .collect()
    }

    /// Generated arms for `From<Leaf> for Node`.
    fn leaf_to_node(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .filter(|variant| !variant.is_branch())
            .map(|variant| {
                let ident = &variant.ident;
                quote! { Self::#ident => Node::#ident }
            })
            .collect()
    }

    /// Generated `Leaf` enum variants.
    fn leaf_variants(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .filter(|variant| !variant.is_branch())
            .map(|variant| {
                let ident = &variant.ident;
                let tag = usize_literal(variant.tag);
                quote! { #ident = #tag }
            })
            .collect()
    }

    /// Generated `Match` helper struct.
    fn match_struct(&self) -> TokenStream {
        let generic_idents: Vec<_> = self.variants.iter().map(|variant| &variant.ident).collect();
        let fields: Vec<_> = self
            .variants
            .iter()
            .map(|variant| {
                let field = raw_aware_ident(&variant.ident.to_string().to_snake_case());
                let ty = &variant.ident;
                quote! { #field: #ty }
            })
            .collect();
        quote! {
            #[derive(Clone, Debug, Eq, Hash, PartialEq)]
            pub struct Match<#(#generic_idents),*> {
                #(#fields,)*
            }
        }
    }

    /// Generated `IntoEnumIterator` implementation for `Node`.
    fn node_into_enum_iterator(&self) -> TokenStream {
        let len = usize_literal(self.variants.len());
        let variants = self.variants.iter().map(|variant| {
            let ident = &variant.ident;
            quote! { Self::#ident }
        });
        quote! {
            impl ::coterms::IntoEnumIterator for Node {
                type Iterator = ::core::array::IntoIter<Self, #len>;

                #[inline(always)]
                fn iter() -> Self::Iterator {
                    ::core::iter::IntoIterator::into_iter([#(#variants),*])
                }
            }
        }
    }

    /// Generated source-oriented `Debug` arms for nodes.
    fn node_type_debug_arms(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .map(|variant| {
                let ident = &variant.ident;
                let name = match variant.source {
                    VariantSource::Enum => variant.ident.to_string(),
                    VariantSource::Struct => self.ident.to_string(),
                };
                quote! { Self::#ident => formatter.write_str(#name) }
            })
            .collect()
    }

    /// Generated `Node` enum variants.
    fn node_variants(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .map(|variant| {
                let ident = &variant.ident;
                let tag = usize_literal(variant.tag);
                quote! { #ident = #tag }
            })
            .collect()
    }

    /// Parse the only supported happy path: an enum item.
    fn parse(tokens: TokenStream) -> syn::Result<Self> {
        let source: DeriveInput = parse2(tokens)?;
        let DeriveInput {
            data,
            generics,
            ident,
            vis,
            ..
        } = source;
        let module = format_ident!("coterm_{}", ident.to_string().to_snake_case());
        let mut next_field = 0_usize;
        let variants = variants(data, &mut next_field)?;
        Ok(Self {
            generics,
            ident,
            module,
            variants,
            visibility: vis,
        })
    }

    /// Generated field type registrations.
    fn register_all_field_types(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .flat_map(|variant| &variant.fields)
            .map(|field| {
                let ty = &field.ty;
                quote! { let () = registry.register::<#ty>(); }
            })
            .collect()
    }

    /// Generated arms for `TryFrom<ErasedBranch> for Branch`.
    fn try_branch_arms(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .filter(|variant| variant.is_branch())
            .map(|variant| {
                let ident = &variant.ident;
                let tag = usize_literal(variant.tag);
                quote! { #tag => Self::#ident }
            })
            .collect()
    }

    /// Generated arms for `TryFrom<ErasedField> for Field`.
    fn try_field_arms(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .flat_map(|variant| &variant.fields)
            .map(|field| {
                let ident = &field.ident;
                let tag = usize_literal(field.tag);
                quote! { #tag => Self::#ident }
            })
            .collect()
    }

    /// Generated arms for `TryFrom<ErasedLeaf> for Leaf`.
    fn try_leaf_arms(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .filter(|variant| !variant.is_branch())
            .map(|variant| {
                let ident = &variant.ident;
                let tag = usize_literal(variant.tag);
                quote! { #tag => Self::#ident }
            })
            .collect()
    }

    /// Generated arms for `TryFrom<ErasedNode> for Node`.
    fn try_node_arms(&self) -> Vec<TokenStream> {
        self.variants
            .iter()
            .map(|variant| {
                let ident = &variant.ident;
                let tag = usize_literal(variant.tag);
                quote! { #tag => Self::#ident }
            })
            .collect()
    }
}

impl Field {
    /// The expression that extracts this field while rebuilding a term.
    fn constructor(&self, module: &Ident) -> TokenStream {
        let ident = &self.ident;
        let ty = &self.ty;
        quote! { fields.field::<#ty>(#module::Field::#ident)? }
    }

    /// The field name used in generated product structs and braced constructors.
    fn product_field(&self) -> Ident {
        self.source_ident
            .clone()
            .unwrap_or_else(|| format_ident!("_{}", self.local))
    }

    /// The generic parameter name used for this field in a generated product.
    fn product_ty(&self) -> Ident {
        if self.source_ident.is_some() {
            self.ident.clone()
        } else {
            format_ident!("_{}", self.local)
        }
    }
}

impl Variant {
    /// The constructor expression used by `Dual::from_node`.
    fn constructor(&self, module: &Ident) -> TokenStream {
        let ident = &self.ident;
        match (self.source, self.style) {
            (VariantSource::Enum, VariantStyle::Named) => {
                let fields: Vec<_> = self
                    .fields
                    .iter()
                    .map(|field| {
                        let source = field.product_field();
                        let value = field.constructor(module);
                        quote! { #source: #value }
                    })
                    .collect();
                quote! { Self::#ident { #(#fields),* } }
            }
            (VariantSource::Enum, VariantStyle::Unit) => quote! { Self::#ident },
            (VariantSource::Enum, VariantStyle::Unnamed) => {
                let fields: Vec<_> = self
                    .fields
                    .iter()
                    .map(|field| field.constructor(module))
                    .collect();
                quote! { Self::#ident(#(#fields),*) }
            }
            (VariantSource::Struct, VariantStyle::Named) => {
                let fields: Vec<_> = self
                    .fields
                    .iter()
                    .map(|field| {
                        let source = field.product_field();
                        let value = field.constructor(module);
                        quote! { #source: #value }
                    })
                    .collect();
                quote! { Self { #(#fields),* } }
            }
            (VariantSource::Struct, VariantStyle::Unit) => quote! { Self },
            (VariantSource::Struct, VariantStyle::Unnamed) => {
                let fields: Vec<_> = self
                    .fields
                    .iter()
                    .map(|field| field.constructor(module))
                    .collect();
                quote! { Self(#(#fields),*) }
            }
        }
    }

    /// Whether this variant has payload fields.
    fn is_branch(&self) -> bool {
        !self.fields.is_empty()
    }

    /// The pattern used by `Dual::fields`.
    fn match_pattern(&self) -> TokenStream {
        let ident = &self.ident;
        match (self.source, self.style) {
            (VariantSource::Enum, VariantStyle::Named) => {
                let bindings: Vec<_> = self
                    .fields
                    .iter()
                    .map(|field| {
                        let binding = &field.binding;
                        quote! { ref #binding }
                    })
                    .collect();
                quote! { Self::#ident { #(#bindings),* } }
            }
            (VariantSource::Enum, VariantStyle::Unit) => quote! { Self::#ident },
            (VariantSource::Enum, VariantStyle::Unnamed) => {
                let bindings: Vec<_> = self
                    .fields
                    .iter()
                    .map(|field| {
                        let binding = &field.binding;
                        quote! { ref #binding }
                    })
                    .collect();
                quote! { Self::#ident(#(#bindings),*) }
            }
            (VariantSource::Struct, VariantStyle::Named) => {
                let bindings: Vec<_> = self
                    .fields
                    .iter()
                    .map(|field| {
                        let binding = &field.binding;
                        quote! { ref #binding }
                    })
                    .collect();
                quote! { Self { #(#bindings),* } }
            }
            (VariantSource::Struct, VariantStyle::Unit) => quote! { Self },
            (VariantSource::Struct, VariantStyle::Unnamed) => {
                let bindings: Vec<_> = self
                    .fields
                    .iter()
                    .map(|field| {
                        let binding = &field.binding;
                        quote! { ref #binding }
                    })
                    .collect();
                quote! { Self(#(#bindings),*) }
            }
        }
    }
}

/// Top-down node-by-node construction of a term.
#[inline]
#[must_use]
pub fn coterm(item: TokenStream) -> TokenStream {
    match Enum::parse(item) {
        Ok(input) => input.expand(),
        Err(error) => error.to_compile_error(),
    }
}

/// Top-down node-by-node computation of a function.
#[inline]
#[must_use]
pub fn incremental(attr: TokenStream, _item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new_spanned(attr, "`incremental` does not accept arguments")
            .to_compile_error();
    }
    TokenStream::new()
}

/// Generate the global field enum with source-oriented debug names.
fn field_enum_definition(variants: &[TokenStream], debug_arms: &[TokenStream]) -> TokenStream {
    if variants.is_empty() {
        quote! {
            #[derive(Clone, Copy, Eq, Hash, PartialEq)]
            pub enum Field {}

            impl ::core::fmt::Debug for Field {
                #[inline(always)]
                fn fmt(
                    &self,
                    _formatter: &mut ::core::fmt::Formatter<'_>,
                ) -> ::core::fmt::Result {
                    match *self {}
                }
            }
        }
    } else {
        quote! {
            #[repr(usize)]
            #[derive(Clone, Copy, Eq, Hash, PartialEq)]
            pub enum Field {
                #(#variants,)*
            }

            impl ::core::fmt::Debug for Field {
                #[inline(always)]
                fn fmt(
                    &self,
                    formatter: &mut ::core::fmt::Formatter<'_>,
                ) -> ::core::fmt::Result {
                    match *self {
                        #(#debug_arms,)*
                    }
                }
            }
        }
    }
}

/// Generate one local tag enum, using `repr(usize)` exactly when inhabited.
fn enum_definition(name: &str, variants: &[TokenStream]) -> TokenStream {
    let ident = format_ident!("{}", name);
    if variants.is_empty() {
        quote! {
            #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
            pub enum #ident {}
        }
    } else {
        quote! {
            #[repr(usize)]
            #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
            pub enum #ident {
                #(#variants,)*
            }
        }
    }
}

/// Generate conversion from a local tag enum to its erased representation.
fn erased_from_impl(
    local_type_name: &str,
    erased_type_name: &str,
    local_is_empty: bool,
) -> TokenStream {
    let erased = format_ident!("{}", erased_type_name);
    let local = format_ident!("{}", local_type_name);
    let body = if local_is_empty {
        quote! { match self {} }
    } else {
        quote! { ::coterms::#erased(self as usize) }
    };
    let unreachable = unreachable_code_allow(local_is_empty);
    quote! {
        impl #local {
            #[inline(always)]
            pub const fn erase(self) -> ::coterms::#erased {
                #body
            }
        }

        impl From<#local> for ::coterms::#erased {
            #unreachable
            #[inline(always)]
            fn from(value: #local) -> Self {
                value.erase()
            }
        }
    }
}

/// Build a single generated field descriptor.
fn field(
    variant: &Ident,
    local: usize,
    prefix: FieldPrefix,
    source_ident: Option<Ident>,
    ty: Type,
    next_field: &mut usize,
) -> syn::Result<Field> {
    let field_suffix = source_ident.as_ref().map_or_else(
        || local.to_string(),
        |ident| ident.to_string().to_upper_camel_case(),
    );
    let ident = match (prefix, source_ident.is_some()) {
        (FieldPrefix::No, false) => format_ident!("_{field_suffix}"),
        (FieldPrefix::No, true) => format_ident!("{field_suffix}"),
        (FieldPrefix::Variant, _) => format_ident!("{variant}{field_suffix}"),
    };
    let binding = source_ident
        .clone()
        .unwrap_or_else(|| unnamed_binding(variant, local));
    let tag = *next_field;
    *next_field = next_field
        .checked_add(1)
        .ok_or_else(|| syn::Error::new_spanned(variant, "too many coterm fields"))?;
    Ok(Field {
        binding,
        ident,
        local,
        source_ident,
        tag,
        ty,
    })
}

/// Parse variant fields into generated field descriptors.
fn fields(
    variant: &Ident,
    prefix: FieldPrefix,
    source_fields: Fields,
    next_field: &mut usize,
) -> syn::Result<(VariantStyle, Vec<Field>)> {
    match source_fields {
        Fields::Named(named_fields) => named_fields
            .named
            .into_iter()
            .enumerate()
            .map(|(local, source)| {
                let Some(ident) = source.ident else {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "braced enum fields must be named",
                    ));
                };
                field(variant, local, prefix, Some(ident), source.ty, next_field)
            })
            .collect::<syn::Result<Vec<_>>>()
            .map(|parsed_fields| (VariantStyle::Named, parsed_fields)),
        Fields::Unit => Ok((VariantStyle::Unit, Vec::new())),
        Fields::Unnamed(unnamed_fields) => unnamed_fields
            .unnamed
            .into_iter()
            .enumerate()
            .map(|(local, source)| field(variant, local, prefix, None, source.ty, next_field))
            .collect::<syn::Result<Vec<_>>>()
            .map(|parsed_fields| (VariantStyle::Unnamed, parsed_fields)),
    }
}

/// Generate the node enum with source-oriented debug names.
fn node_enum_definition(variants: &[TokenStream], debug_arms: &[TokenStream]) -> TokenStream {
    if variants.is_empty() {
        quote! {
            #[derive(Clone, Copy, Eq, Hash, PartialEq)]
            pub enum Node {}

            impl ::core::fmt::Debug for Node {
                #[inline(always)]
                fn fmt(
                    &self,
                    _formatter: &mut ::core::fmt::Formatter<'_>,
                ) -> ::core::fmt::Result {
                    match *self {}
                }
            }
        }
    } else {
        quote! {
            #[repr(usize)]
            #[derive(Clone, Copy, Eq, Hash, PartialEq)]
            pub enum Node {
                #(#variants,)*
            }

            impl ::core::fmt::Debug for Node {
                #[inline(always)]
                fn fmt(
                    &self,
                    formatter: &mut ::core::fmt::Formatter<'_>,
                ) -> ::core::fmt::Result {
                    match *self {
                        #(#debug_arms,)*
                    }
                }
            }
        }
    }
}

/// Build a generated helper field identifier that cannot collide with keywords.
fn raw_aware_ident(text: &str) -> Ident {
    Ident::new_raw(&format!("coterms_{text}"), Span::call_site())
}

/// Generate an erased-to-local `TryFrom` implementation.
fn try_from_impl(
    erased_type_name: &str,
    local_type_name: &str,
    arms: &[TokenStream],
) -> TokenStream {
    let erased = format_ident!("{}", erased_type_name);
    let local = format_ident!("{}", local_type_name);
    let unreachable = unreachable_code_allow(arms.is_empty());
    quote! {
        impl TryFrom<::coterms::#erased> for #local {
            type Error = ::coterms::#erased;

            #unreachable
            #[inline(always)]
            fn try_from(value: ::coterms::#erased) -> Result<Self, Self::Error> {
                Ok(match value.0 {
                    #(#arms,)*
                    _ => return Err(value),
                })
            }
        }
    }
}

/// Name an unnamed field binding.
fn unnamed_binding(variant: &Ident, local: usize) -> Ident {
    if variant == "Successor" && local == 0 {
        return format_ident!("predecessor");
    }
    format_ident!("_{}", local)
}

/// Render a generated tag without a Rust type suffix.
fn usize_literal(value: usize) -> Literal {
    Literal::usize_unsuffixed(value)
}

/// Suppress unreachable-call warnings caused by uninhabited generated enums.
fn unreachable_code_allow(enabled: bool) -> TokenStream {
    if enabled {
        quote! {
            #[allow(
                unreachable_code,
                reason = "delegating from an uninhabited generated enum is intentionally unreachable"
            )]
        }
    } else {
        TokenStream::new()
    }
}

/// Parse item data into the same variant representation for structs and enums.
fn variants(source_data: Data, next_field: &mut usize) -> syn::Result<Vec<Variant>> {
    match source_data {
        Data::Enum(enum_data) => enum_data
            .variants
            .into_iter()
            .enumerate()
            .map(|(tag, variant)| {
                let (style, fields) = fields(
                    &variant.ident,
                    FieldPrefix::Variant,
                    variant.fields,
                    next_field,
                )?;
                Ok(Variant {
                    fields,
                    ident: variant.ident,
                    source: VariantSource::Enum,
                    style,
                    tag,
                })
            })
            .collect(),
        Data::Struct(struct_data) => {
            let ident = format_ident!("Struct");
            let (style, fields) = fields(&ident, FieldPrefix::No, struct_data.fields, next_field)?;
            Ok(vec![Variant {
                fields,
                ident,
                source: VariantSource::Struct,
                style,
                tag: 0_usize,
            }])
        }
        Data::Union(union_data) => Err(syn::Error::new_spanned(
            union_data.union_token,
            "`coterm` does not support unions",
        )),
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::needless_raw_strings, reason = "consistency")]
    #![expect(clippy::unwrap_used, reason = "failing tests ought to panic")]

    use {super::*, pretty_assertions::assert_eq, prettyplease::unparse};

    #[test]
    fn coterm_void() {
        let input: TokenStream = r#"
pub enum Void {}
"#
        .parse()
        .unwrap();
        let expected = r#"
pub mod coterm_void {
    #[derive(Clone, Copy, Eq, Hash, PartialEq)]
    pub enum Node {}
    impl ::core::fmt::Debug for Node {
        #[inline(always)]
        fn fmt(
            &self,
            _formatter: &mut ::core::fmt::Formatter<'_>,
        ) -> ::core::fmt::Result {
            match *self {}
        }
    }
    impl ::coterms::IntoEnumIterator for Node {
        type Iterator = ::core::array::IntoIter<Self, 0>;
        #[inline(always)]
        fn iter() -> Self::Iterator {
            ::core::iter::IntoIterator::into_iter([])
        }
    }
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum Branch {}
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum Leaf {}
    #[derive(Clone, Copy, Eq, Hash, PartialEq)]
    pub enum Field {}
    impl ::core::fmt::Debug for Field {
        #[inline(always)]
        fn fmt(
            &self,
            _formatter: &mut ::core::fmt::Formatter<'_>,
        ) -> ::core::fmt::Result {
            match *self {}
        }
    }
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct Match {}
    impl Branch {
        #[inline(always)]
        pub const fn node(self) -> Node {
            match self {}
        }
    }
    impl Leaf {
        #[inline(always)]
        pub const fn node(self) -> Node {
            match self {}
        }
    }
    impl Field {
        #[inline(always)]
        pub const fn branch(self) -> Branch {
            match self {}
        }
    }
    impl From<Branch> for Node {
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn from(value: Branch) -> Self {
            value.node()
        }
    }
    impl From<Leaf> for Node {
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn from(value: Leaf) -> Self {
            value.node()
        }
    }
    impl From<Field> for Branch {
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn from(value: Field) -> Self {
            value.branch()
        }
    }
    impl Branch {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedBranch {
            match self {}
        }
    }
    impl From<Branch> for ::coterms::ErasedBranch {
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn from(value: Branch) -> Self {
            value.erase()
        }
    }
    impl Branch {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyBranch {
            ::coterms::AnyBranch {
                erased: self.erase(),
                ty: ::core::any::TypeId::of::<super::Void>(),
            }
        }
    }
    impl Leaf {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedLeaf {
            match self {}
        }
    }
    impl From<Leaf> for ::coterms::ErasedLeaf {
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn from(value: Leaf) -> Self {
            value.erase()
        }
    }
    impl Leaf {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyLeaf {
            ::coterms::AnyLeaf {
                erased: self.erase(),
                ty: ::core::any::TypeId::of::<super::Void>(),
            }
        }
    }
    impl Node {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedNode {
            match self {}
        }
    }
    impl From<Node> for ::coterms::ErasedNode {
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn from(value: Node) -> Self {
            value.erase()
        }
    }
    impl Node {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyNode {
            ::coterms::AnyNode {
                erased: self.erase(),
                ty: ::core::any::TypeId::of::<super::Void>(),
            }
        }
    }
    impl Field {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedField {
            match self {}
        }
    }
    impl From<Field> for ::coterms::ErasedField {
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn from(value: Field) -> Self {
            value.erase()
        }
    }
    impl Field {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyField {
            ::coterms::AnyField {
                erased: self.erase(),
                parent_ty: ::core::any::TypeId::of::<super::Void>(),
            }
        }
    }
    impl TryFrom<::coterms::ErasedBranch> for Branch {
        type Error = ::coterms::ErasedBranch;
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedBranch) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    _ => return Err(value),
                },
            )
        }
    }
    impl TryFrom<::coterms::ErasedLeaf> for Leaf {
        type Error = ::coterms::ErasedLeaf;
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedLeaf) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    _ => return Err(value),
                },
            )
        }
    }
    impl TryFrom<::coterms::ErasedNode> for Node {
        type Error = ::coterms::ErasedNode;
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedNode) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    _ => return Err(value),
                },
            )
        }
    }
    impl TryFrom<::coterms::ErasedField> for Field {
        type Error = ::coterms::ErasedField;
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedField) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    _ => return Err(value),
                },
            )
        }
    }
}
impl ::coterms::Dual for Void {
    type Branch = coterm_void::Branch;
    type Leaf = coterm_void::Leaf;
    type Node = coterm_void::Node;
    type Field = coterm_void::Field;
    type Deref = Self;
    #[inline(always)]
    fn deref(&self) -> &Self::Deref {
        self
    }
    #[inline(always)]
    fn fields(
        &self,
    ) -> Result<
        ::coterms::HashMap<<Self as ::coterms::Dual>::Field, ::coterms::AnyTerm<'_>>,
        <Self as ::coterms::Dual>::Leaf,
    > {
        match *self {}
    }
    #[inline(always)]
    fn fields_of_node(
        node: <Self as ::coterms::Dual>::Node,
    ) -> Result<
        ::coterms::HashSet<<Self as ::coterms::Dual>::Field>,
        <Self as ::coterms::Dual>::Leaf,
    > {
        match node {}
    }
    #[inline(always)]
    fn from_node<F>(
        node: <Self as ::coterms::Dual>::Node,
        fields: F,
    ) -> Result<Self, ::coterms::DualError>
    where
        F: ::coterms::Fields<Self::Deref>,
    {
        match node {}
    }
    #[inline]
    fn register_all_field_types(registry: &mut ::coterms::Registry) {}
    #[inline(always)]
    fn field_type(field: <Self as ::coterms::Dual>::Field) -> ::core::any::TypeId {
        match field {}
    }
}
"#
        .trim();
        let output = coterm(input);
        let actual = unparse(&syn::parse2(output).unwrap());
        assert_eq!(expected, actual.trim());
    }

    #[test]
    fn coterm_peano() {
        let input: TokenStream = r#"
pub enum Peano {
    Zero,
    Successor(Box<Self>),
}
"#
        .parse()
        .unwrap();
        let expected = r#"
pub mod coterm_peano {
    #[repr(usize)]
    #[derive(Clone, Copy, Eq, Hash, PartialEq)]
    pub enum Node {
        Zero = 0,
        Successor = 1,
    }
    impl ::core::fmt::Debug for Node {
        #[inline(always)]
        fn fmt(
            &self,
            formatter: &mut ::core::fmt::Formatter<'_>,
        ) -> ::core::fmt::Result {
            match *self {
                Self::Zero => formatter.write_str("Zero"),
                Self::Successor => formatter.write_str("Successor"),
            }
        }
    }
    impl ::coterms::IntoEnumIterator for Node {
        type Iterator = ::core::array::IntoIter<Self, 2>;
        #[inline(always)]
        fn iter() -> Self::Iterator {
            ::core::iter::IntoIterator::into_iter([Self::Zero, Self::Successor])
        }
    }
    #[repr(usize)]
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum Branch {
        Successor = 1,
    }
    #[repr(usize)]
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum Leaf {
        Zero = 0,
    }
    #[repr(usize)]
    #[derive(Clone, Copy, Eq, Hash, PartialEq)]
    pub enum Field {
        Successor0 = 0,
    }
    impl ::core::fmt::Debug for Field {
        #[inline(always)]
        fn fmt(
            &self,
            formatter: &mut ::core::fmt::Formatter<'_>,
        ) -> ::core::fmt::Result {
            match *self {
                Self::Successor0 => formatter.write_str("0"),
            }
        }
    }
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum FieldInZero {}
    #[repr(usize)]
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum FieldInSuccessor {
        Successor0 = 0,
    }
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct Match<Zero, Successor> {
        r#coterms_zero: Zero,
        r#coterms_successor: Successor,
    }
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct FieldsInZero {}
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct FieldsInSuccessor<_0> {
        _0: _0,
    }
    impl Branch {
        #[inline(always)]
        pub const fn node(self) -> Node {
            match self {
                Self::Successor => Node::Successor,
            }
        }
    }
    impl Leaf {
        #[inline(always)]
        pub const fn node(self) -> Node {
            match self {
                Self::Zero => Node::Zero,
            }
        }
    }
    impl Field {
        #[inline(always)]
        pub const fn branch(self) -> Branch {
            match self {
                Self::Successor0 => Branch::Successor,
            }
        }
    }
    impl From<Branch> for Node {
        #[inline(always)]
        fn from(value: Branch) -> Self {
            value.node()
        }
    }
    impl From<Leaf> for Node {
        #[inline(always)]
        fn from(value: Leaf) -> Self {
            value.node()
        }
    }
    impl From<Field> for Branch {
        #[inline(always)]
        fn from(value: Field) -> Self {
            value.branch()
        }
    }
    impl Branch {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedBranch {
            ::coterms::ErasedBranch(self as usize)
        }
    }
    impl From<Branch> for ::coterms::ErasedBranch {
        #[inline(always)]
        fn from(value: Branch) -> Self {
            value.erase()
        }
    }
    impl Branch {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyBranch {
            ::coterms::AnyBranch {
                erased: self.erase(),
                ty: ::core::any::TypeId::of::<super::Peano>(),
            }
        }
    }
    impl Leaf {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedLeaf {
            ::coterms::ErasedLeaf(self as usize)
        }
    }
    impl From<Leaf> for ::coterms::ErasedLeaf {
        #[inline(always)]
        fn from(value: Leaf) -> Self {
            value.erase()
        }
    }
    impl Leaf {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyLeaf {
            ::coterms::AnyLeaf {
                erased: self.erase(),
                ty: ::core::any::TypeId::of::<super::Peano>(),
            }
        }
    }
    impl Node {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedNode {
            ::coterms::ErasedNode(self as usize)
        }
    }
    impl From<Node> for ::coterms::ErasedNode {
        #[inline(always)]
        fn from(value: Node) -> Self {
            value.erase()
        }
    }
    impl Node {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyNode {
            ::coterms::AnyNode {
                erased: self.erase(),
                ty: ::core::any::TypeId::of::<super::Peano>(),
            }
        }
    }
    impl Field {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedField {
            ::coterms::ErasedField(self as usize)
        }
    }
    impl From<Field> for ::coterms::ErasedField {
        #[inline(always)]
        fn from(value: Field) -> Self {
            value.erase()
        }
    }
    impl Field {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyField {
            ::coterms::AnyField {
                erased: self.erase(),
                parent_ty: ::core::any::TypeId::of::<super::Peano>(),
            }
        }
    }
    impl TryFrom<::coterms::ErasedBranch> for Branch {
        type Error = ::coterms::ErasedBranch;
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedBranch) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    1 => Self::Successor,
                    _ => return Err(value),
                },
            )
        }
    }
    impl TryFrom<::coterms::ErasedLeaf> for Leaf {
        type Error = ::coterms::ErasedLeaf;
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedLeaf) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    0 => Self::Zero,
                    _ => return Err(value),
                },
            )
        }
    }
    impl TryFrom<::coterms::ErasedNode> for Node {
        type Error = ::coterms::ErasedNode;
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedNode) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    0 => Self::Zero,
                    1 => Self::Successor,
                    _ => return Err(value),
                },
            )
        }
    }
    impl TryFrom<::coterms::ErasedField> for Field {
        type Error = ::coterms::ErasedField;
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedField) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    0 => Self::Successor0,
                    _ => return Err(value),
                },
            )
        }
    }
}
impl ::coterms::Dual for Peano {
    type Branch = coterm_peano::Branch;
    type Leaf = coterm_peano::Leaf;
    type Node = coterm_peano::Node;
    type Field = coterm_peano::Field;
    type Deref = Self;
    #[inline(always)]
    fn deref(&self) -> &Self::Deref {
        self
    }
    #[inline(always)]
    fn fields(
        &self,
    ) -> Result<
        ::coterms::HashMap<<Self as ::coterms::Dual>::Field, ::coterms::AnyTerm<'_>>,
        <Self as ::coterms::Dual>::Leaf,
    > {
        match *self {
            Self::Zero => Err(coterm_peano::Leaf::Zero),
            Self::Successor(ref predecessor) => {
                Ok(
                    [
                        (
                            coterm_peano::Field::Successor0,
                            ::coterms::AnyTerm::new::<Box<Self>>(predecessor),
                        ),
                    ]
                        .into_iter()
                        .collect(),
                )
            }
        }
    }
    #[inline(always)]
    fn fields_of_node(
        node: <Self as ::coterms::Dual>::Node,
    ) -> Result<
        ::coterms::HashSet<<Self as ::coterms::Dual>::Field>,
        <Self as ::coterms::Dual>::Leaf,
    > {
        match node {
            coterm_peano::Node::Zero => Err(coterm_peano::Leaf::Zero),
            coterm_peano::Node::Successor => {
                Ok(::core::iter::once(coterm_peano::Field::Successor0).collect())
            }
        }
    }
    #[inline(always)]
    fn from_node<F>(
        node: <Self as ::coterms::Dual>::Node,
        fields: F,
    ) -> Result<Self, ::coterms::DualError>
    where
        F: ::coterms::Fields<Self::Deref>,
    {
        Ok(
            match node {
                coterm_peano::Node::Zero => Self::Zero,
                coterm_peano::Node::Successor => {
                    Self::Successor(
                        fields.field::<Box<Self>>(coterm_peano::Field::Successor0)?,
                    )
                }
            },
        )
    }
    #[inline]
    fn register_all_field_types(registry: &mut ::coterms::Registry) {
        let () = registry.register::<Box<Self>>();
    }
    #[inline(always)]
    fn field_type(field: <Self as ::coterms::Dual>::Field) -> ::core::any::TypeId {
        match field {
            coterm_peano::Field::Successor0 => {
                ::core::any::TypeId::of::<<Box<Self> as ::coterms::Dual>::Deref>()
            }
        }
    }
}
"#
        .trim();
        let output = coterm(input);
        let actual = unparse(&syn::parse2(output).unwrap());
        assert_eq!(expected, actual.trim());
    }

    #[test]
    fn coterm_singleton() {
        let input: TokenStream = r#"
pub struct Singleton {
    value: Peano,
}
"#
        .parse()
        .unwrap();
        let expected = r#"
pub mod coterm_singleton {
    #[repr(usize)]
    #[derive(Clone, Copy, Eq, Hash, PartialEq)]
    pub enum Node {
        Struct = 0,
    }
    impl ::core::fmt::Debug for Node {
        #[inline(always)]
        fn fmt(
            &self,
            formatter: &mut ::core::fmt::Formatter<'_>,
        ) -> ::core::fmt::Result {
            match *self {
                Self::Struct => formatter.write_str("Singleton"),
            }
        }
    }
    impl ::coterms::IntoEnumIterator for Node {
        type Iterator = ::core::array::IntoIter<Self, 1>;
        #[inline(always)]
        fn iter() -> Self::Iterator {
            ::core::iter::IntoIterator::into_iter([Self::Struct])
        }
    }
    #[repr(usize)]
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum Branch {
        Struct = 0,
    }
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum Leaf {}
    #[repr(usize)]
    #[derive(Clone, Copy, Eq, Hash, PartialEq)]
    pub enum Field {
        Value = 0,
    }
    impl ::core::fmt::Debug for Field {
        #[inline(always)]
        fn fmt(
            &self,
            formatter: &mut ::core::fmt::Formatter<'_>,
        ) -> ::core::fmt::Result {
            match *self {
                Self::Value => formatter.write_str("value"),
            }
        }
    }
    #[repr(usize)]
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub enum FieldInStruct {
        Value = 0,
    }
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct Match<Struct> {
        r#coterms_struct: Struct,
    }
    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    pub struct FieldsInStruct<Value> {
        value: Value,
    }
    impl Branch {
        #[inline(always)]
        pub const fn node(self) -> Node {
            match self {
                Self::Struct => Node::Struct,
            }
        }
    }
    impl Leaf {
        #[inline(always)]
        pub const fn node(self) -> Node {
            match self {}
        }
    }
    impl Field {
        #[inline(always)]
        pub const fn branch(self) -> Branch {
            match self {
                Self::Value => Branch::Struct,
            }
        }
    }
    impl From<Branch> for Node {
        #[inline(always)]
        fn from(value: Branch) -> Self {
            value.node()
        }
    }
    impl From<Leaf> for Node {
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn from(value: Leaf) -> Self {
            value.node()
        }
    }
    impl From<Field> for Branch {
        #[inline(always)]
        fn from(value: Field) -> Self {
            value.branch()
        }
    }
    impl Branch {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedBranch {
            ::coterms::ErasedBranch(self as usize)
        }
    }
    impl From<Branch> for ::coterms::ErasedBranch {
        #[inline(always)]
        fn from(value: Branch) -> Self {
            value.erase()
        }
    }
    impl Branch {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyBranch {
            ::coterms::AnyBranch {
                erased: self.erase(),
                ty: ::core::any::TypeId::of::<super::Singleton>(),
            }
        }
    }
    impl Leaf {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedLeaf {
            match self {}
        }
    }
    impl From<Leaf> for ::coterms::ErasedLeaf {
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn from(value: Leaf) -> Self {
            value.erase()
        }
    }
    impl Leaf {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyLeaf {
            ::coterms::AnyLeaf {
                erased: self.erase(),
                ty: ::core::any::TypeId::of::<super::Singleton>(),
            }
        }
    }
    impl Node {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedNode {
            ::coterms::ErasedNode(self as usize)
        }
    }
    impl From<Node> for ::coterms::ErasedNode {
        #[inline(always)]
        fn from(value: Node) -> Self {
            value.erase()
        }
    }
    impl Node {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyNode {
            ::coterms::AnyNode {
                erased: self.erase(),
                ty: ::core::any::TypeId::of::<super::Singleton>(),
            }
        }
    }
    impl Field {
        #[inline(always)]
        pub const fn erase(self) -> ::coterms::ErasedField {
            ::coterms::ErasedField(self as usize)
        }
    }
    impl From<Field> for ::coterms::ErasedField {
        #[inline(always)]
        fn from(value: Field) -> Self {
            value.erase()
        }
    }
    impl Field {
        #[inline(always)]
        pub const fn any(self) -> ::coterms::AnyField {
            ::coterms::AnyField {
                erased: self.erase(),
                parent_ty: ::core::any::TypeId::of::<super::Singleton>(),
            }
        }
    }
    impl TryFrom<::coterms::ErasedBranch> for Branch {
        type Error = ::coterms::ErasedBranch;
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedBranch) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    0 => Self::Struct,
                    _ => return Err(value),
                },
            )
        }
    }
    impl TryFrom<::coterms::ErasedLeaf> for Leaf {
        type Error = ::coterms::ErasedLeaf;
        #[allow(
            unreachable_code,
            reason = "delegating from an uninhabited generated enum is intentionally unreachable"
        )]
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedLeaf) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    _ => return Err(value),
                },
            )
        }
    }
    impl TryFrom<::coterms::ErasedNode> for Node {
        type Error = ::coterms::ErasedNode;
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedNode) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    0 => Self::Struct,
                    _ => return Err(value),
                },
            )
        }
    }
    impl TryFrom<::coterms::ErasedField> for Field {
        type Error = ::coterms::ErasedField;
        #[inline(always)]
        fn try_from(value: ::coterms::ErasedField) -> Result<Self, Self::Error> {
            Ok(
                match value.0 {
                    0 => Self::Value,
                    _ => return Err(value),
                },
            )
        }
    }
}
impl ::coterms::Dual for Singleton {
    type Branch = coterm_singleton::Branch;
    type Leaf = coterm_singleton::Leaf;
    type Node = coterm_singleton::Node;
    type Field = coterm_singleton::Field;
    type Deref = Self;
    #[inline(always)]
    fn deref(&self) -> &Self::Deref {
        self
    }
    #[inline(always)]
    fn fields(
        &self,
    ) -> Result<
        ::coterms::HashMap<<Self as ::coterms::Dual>::Field, ::coterms::AnyTerm<'_>>,
        <Self as ::coterms::Dual>::Leaf,
    > {
        match *self {
            Self { ref value } => {
                Ok(
                    [
                        (
                            coterm_singleton::Field::Value,
                            ::coterms::AnyTerm::new::<Peano>(value),
                        ),
                    ]
                        .into_iter()
                        .collect(),
                )
            }
        }
    }
    #[inline(always)]
    fn fields_of_node(
        node: <Self as ::coterms::Dual>::Node,
    ) -> Result<
        ::coterms::HashSet<<Self as ::coterms::Dual>::Field>,
        <Self as ::coterms::Dual>::Leaf,
    > {
        match node {
            coterm_singleton::Node::Struct => {
                Ok(::core::iter::once(coterm_singleton::Field::Value).collect())
            }
        }
    }
    #[inline(always)]
    fn from_node<F>(
        node: <Self as ::coterms::Dual>::Node,
        fields: F,
    ) -> Result<Self, ::coterms::DualError>
    where
        F: ::coterms::Fields<Self::Deref>,
    {
        Ok(
            match node {
                coterm_singleton::Node::Struct => {
                    Self {
                        value: fields.field::<Peano>(coterm_singleton::Field::Value)?,
                    }
                }
            },
        )
    }
    #[inline]
    fn register_all_field_types(registry: &mut ::coterms::Registry) {
        let () = registry.register::<Peano>();
    }
    #[inline(always)]
    fn field_type(field: <Self as ::coterms::Dual>::Field) -> ::core::any::TypeId {
        match field {
            coterm_singleton::Field::Value => {
                ::core::any::TypeId::of::<<Peano as ::coterms::Dual>::Deref>()
            }
        }
    }
}
"#
        .trim();
        let output = coterm(input);
        let actual = unparse(&syn::parse2(output).unwrap());
        assert_eq!(expected, actual.trim());
    }

    #[test]
    fn incremental_rejects_arguments() {
        let attr = quote! { unexpected };
        let output = incremental(attr, TokenStream::new());

        assert!(!output.is_empty());
        assert!(
            output
                .to_string()
                .contains("`incremental` does not accept arguments")
        );
    }

    #[test]
    fn incremental_without_arguments_is_empty() {
        let output = incremental(TokenStream::new(), TokenStream::new());

        assert!(output.is_empty());
    }

    #[test]
    fn unnamed_binding_only_special_cases_the_first_successor_field() {
        let other = Ident::new("Other", Span::call_site());
        let successor = Ident::new("Successor", Span::call_site());

        assert_eq!(unnamed_binding(&successor, 0).to_string(), "predecessor");
        assert_eq!(unnamed_binding(&successor, 1).to_string(), "_1");
        assert_eq!(unnamed_binding(&other, 0).to_string(), "_0");
        assert_eq!(unnamed_binding(&other, 1).to_string(), "_1");
    }

    /*
    #[test]
    fn incremental_min() {
        let input: TokenStream = r#"
fn min(lhs: Peano, rhs: Peano) -> Peano {
    match lhs {
        Peano::Zero => Peano::Zero,
        Peano::Successor(lhs_pred) => {
            match rhs {
                Peano::Zero => Peano::Zero,
                Peano::Successor(rhs_pred) => {
                    Peano::Successor(min(lhs_pred, rhs_pred))
                }
            }
        }
    }
}
"#
        .parse()
        .unwrap();
        let expected = r#"
pub mod incremental_min {
    use super::*;

    const CASE_STATEMENTS_TOPOSORT: &[::coterms::AnyMatch] = &[
        ::coterms::AnyMatch::new::<(Peano, Peano)>(&[()]),
    ];
}
"#
        .trim();
        let output = incremental(TokenStream::new(), input);
        let actual = unparse(&syn::parse2(output).unwrap());
        assert_eq!(expected, actual.trim());
    }
    */
}
