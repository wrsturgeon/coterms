//! Compile and runtime coverage for externally derived `Dual` implementations.

#[cfg(test)]
mod tests {
    use {
        core::fmt,
        coterms::{Dual, Frontier, from_pinned, pin_up},
    };

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
