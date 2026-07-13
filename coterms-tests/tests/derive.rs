//! Compile and runtime coverage for externally derived `Dual` implementations.

#[cfg(test)]
mod tests {
    #![expect(clippy::expect_used, reason = "failing tests ought to panic")]

    extern crate alloc;

    use {
        alloc::sync::Arc,
        core::{any::TypeId, fmt},
        coterms::{AnyNode, Dual, DualError, Frontier, REGISTRY, from_pinned, pin_up, register},
    };

    /// Constructors whose declaration order differs from lexical order.
    #[expect(
        clippy::arbitrary_source_item_ordering,
        reason = "constructor enumeration must preserve declaration order rather than lexical order"
    )]
    #[derive(Clone, Debug, Dual, Eq, PartialEq)]
    enum NotAlphabeticallySorted {
        /// The first declared constructor.
        B,
        /// The second declared constructor.
        A,
        /// The third declared constructor.
        C,
    }

    /// An enum covering named, unnamed, and unit variants.
    #[derive(Clone, Debug, Dual, Eq, PartialEq)]
    enum EnumShapes<T> {
        /// A variant with named fields.
        Named {
            /// A generic field.
            first: T,
            /// A concrete sibling field.
            second: bool,
        },
        /// A variant with unnamed fields, including recursive structure.
        Tuple(T, Box<Self>),
        /// A variant without fields.
        Unit,
    }

    /// A struct with named fields.
    #[derive(Clone, Debug, Dual, Eq, PartialEq)]
    struct NamedStruct {
        /// A non-unit field.
        left: bool,
        /// A unit field.
        right: (),
    }

    /// A struct with unnamed fields.
    #[derive(Clone, Debug, Dual, Eq, PartialEq)]
    struct TupleStruct(bool, ());

    /// A struct without fields.
    #[derive(Clone, Debug, Dual, Eq, PartialEq)]
    struct UnitStruct;

    /// A registered type with no constructors.
    #[derive(Clone, Debug, Dual, Eq, PartialEq)]
    enum Void {}

    /// A unique type that no test registers.
    struct Unregistered;

    /// Checks both complete-frontier and pinned-map reconstruction.
    fn assert_round_trips<D>(value: &D)
    where
        D: Dual + fmt::Debug + Eq,
    {
        let complete_round_trip = Frontier::complete(value).dual();
        assert_eq!(
            complete_round_trip.as_ref(),
            Ok(value),
            "complete frontier failed to round-trip {value:?}"
        );

        let pinned_round_trip = pin_up(value).and_then(|pinned| from_pinned(&pinned));
        assert_eq!(
            pinned_round_trip.as_ref(),
            Ok(value),
            "pinned map failed to round-trip {value:?}"
        );
    }

    #[test]
    fn enum_shapes_round_trip() {
        assert_round_trips(&EnumShapes::Named {
            first: (),
            second: true,
        });
        assert_round_trips(&EnumShapes::Tuple((), Box::new(EnumShapes::Unit)));
        assert_round_trips(&EnumShapes::<()>::Unit);
    }

    #[test]
    fn constructor_enumeration_preserves_declaration_order_and_shares_storage() {
        let () = register::<NotAlphabeticallySorted>();
        let registry = REGISTRY
            .read()
            .expect("a constructor-enumeration test must not poison the registry");
        let first = registry
            .constructors(TypeId::of::<NotAlphabeticallySorted>())
            .expect("the registered type must expose its constructors");
        let second = registry
            .constructors(TypeId::of::<NotAlphabeticallySorted>())
            .expect("repeated enumeration must succeed");
        let expected: [AnyNode; 3] = [
            <NotAlphabeticallySorted as Dual>::Node::B.any(),
            <NotAlphabeticallySorted as Dual>::Node::A.any(),
            <NotAlphabeticallySorted as Dual>::Node::C.any(),
        ];

        assert_eq!(first.as_ref(), expected);
        assert!(
            Arc::ptr_eq(&first, &second),
            "repeated enumeration must share the registered constructor allocation"
        );
    }

    #[test]
    fn registered_uninhabited_type_has_no_constructors() {
        let () = register::<Void>();
        let registry = REGISTRY
            .read()
            .expect("a constructor-enumeration test must not poison the registry");
        let constructors = registry
            .constructors(TypeId::of::<Void>())
            .expect("a registered uninhabited type must be distinguishable from an unknown type");

        assert!(constructors.is_empty());
    }

    #[test]
    fn unregistered_type_fails_loudly() {
        let ty = TypeId::of::<Unregistered>();
        let registry = REGISTRY
            .read()
            .expect("a constructor-enumeration test must not poison the registry");

        assert_eq!(
            registry.constructors(ty),
            Err(DualError::UnregisteredType(ty))
        );
    }

    #[test]
    fn named_struct_round_trips() {
        assert_round_trips(&NamedStruct {
            left: true,
            right: (),
        });
    }

    #[test]
    fn tuple_struct_round_trips() {
        assert_round_trips(&TupleStruct(false, ()));
    }

    #[test]
    fn unit_struct_round_trips() {
        assert_round_trips(&UnitStruct);
    }
}
