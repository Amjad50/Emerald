#[macro_export]
macro_rules! cmdline_struct {
    // Macro entry point, parsing the struct and calling `build_struct` and `build_parser`
    (
        $(#[$struct_attr: meta])*
        $vis:vis struct $struct_name: ident $(< $lt: lifetime >)? {
            $(
                $(#[$($field_attrs: tt)*])*
                $f_vis:vis $field_name: ident: $field_type: ty
            ),* $(,)?
        }
    ) => {
        $crate::cmdline_struct!(@build_struct $vis [$struct_name $(< $lt >)?] {
            $(
                $(#[$($field_attrs)*])*
                $f_vis $field_name: $field_type,
            )*
        } => [$(#[$struct_attr])*] => []);

        $crate::cmdline_struct!(@build_parser [$struct_name $(< $lt >)?] {
            $(
                $(#[$($field_attrs)*])*
                $field_name: $field_type,
            )*
        });
    };

    // build the struct using recursion, as we need to have the fields prepared in 1 go
    // this is the base case, where we have no more fields to process
    // `$attribs` are the struct attributes from outside, and `$built` are the fields we have processed
    (
        @build_struct $vis:vis [$($name_ident: tt)*] { } => [$($attribs: tt)*] => [$($built: tt)*]
    ) => {
        $($attribs)*
        $vis struct $($name_ident)* {
            $($built)*
        }
    };
    // the branch for handling the `default` custom attribute
    // since this is our attribute, this is not useful for fields generation
    // its only used in `build_parser` to generate the default value
    // we just ignore it here
    (
        @build_struct $vis:vis [$($name_ident: tt)*] {
            #[default = $default: expr]
            $($rest:tt)*
        } => [$($attribs: tt)*] => [$($built: tt)*]
    ) => {
        $crate::cmdline_struct!(@build_struct $vis [$($name_ident)*] {
            $($rest)*
        } => [$($attribs)*] => [$($built)*]);
    };
    // handle field with custom attributes
    // these are not our attributes (everything we handle should be before this)
    // so we just pass them through
    (
        @build_struct $vis:vis [$($name_ident: tt)*] {
            #[$($field_attrs: tt)*]
            $($rest:tt)*
        } => [$($attribs: tt)*] => [$($built: tt)*]
    ) => {
        $crate::cmdline_struct!(@build_struct $vis [$($name_ident)*] {
            $($rest)*
        } => [$($attribs)*] => [
            $($built)*
            #[$($field_attrs)*]
            // $field_name: $field_type,
        ]);
    };
    // handle fields
    // this is the main part of the struct fields generation
    // we put the field in the `$built` list
    // if it had attributes, it will be put before it by the above branch
    (
        @build_struct $vis:vis [$($name_ident: tt)*] {
            $f_vis:vis $field_name: ident: $field_type: ty,
            $($rest:tt)*
        } => [$($attribs: tt)*]  => [$($built: tt)*]
    ) => {
        $crate::cmdline_struct!(@build_struct $vis [$($name_ident)*] {
            $($rest)*
        } => [$($attribs)*] => [
            $($built)*
            $f_vis $field_name: $field_type,
        ]);
    };

    // build the parser for the struct
    // we generate the `parse_cmdline` function for the struct
    // first, we create all the fields, with the same name as its defined
    // in the struct, we use the `Default` trait to create the default value
    //
    // users can then add the `default` attribute to change the default value
    //
    // after that, we loop through the tokens, and match the field name
    // and call the appropriate `parse_cmdline` function for the field
    //
    // for each type used, it must implement `Default` and `CmdlineParse`,
    // where `CmdlineParse` will use the `tokenizer.next_value` to get the string
    // value and parse it as needed.
    (
        @build_parser [$($name_ident: tt)*] {
            $(
                $(#[$($field_attrs: tt)*])*
                $field_name: ident: $field_type: ty,
            )*
        }
    ) => {
        impl<'a> CmdlineParse<'a> for $($name_ident)* {
            #[allow(unused_assignments)]
            fn parse_cmdline(tokenizer: &mut Tokenizer<'a>) -> parser::Result<'a, Self> {
                $(
                    let mut $field_name = Default::default();
                )*
                $(
                    $(
                        $crate::cmdline_struct!(
                            @build_parser_attr
                            #[$($field_attrs)*]
                            $field_name: $field_type
                        );
                    )*
                )*

                while let Some((i, ident)) = tokenizer.next_ident() {
                    match ident {
                        $(
                            stringify!($field_name) => {
                                $field_name = CmdlineParse::parse_cmdline(tokenizer)?;
                            }
                        )*
                        unknown => {
                            return Err(parser::ParseError::new(parser::ParseErrorKind::UnexpectedId(unknown), i))
                        }
                    }
                }

                Ok(Self {
                    $(
                        $field_name,
                    )*
                })
            }
        }
    };

    // this handled the attributes for building the parser
    // the `default` attribute will change the value of `$field_name` to the specified value
    (
        @build_parser_attr
        #[default = $expr: expr]
        $field_name: ident: $field_type: ty
    ) => {
        $field_name = $expr;
    };
    // other attributes are ignored here, they are handled by `build_struct`
    (
        @build_parser_attr
        #[$($field_attrs: tt)*]
        $field_name: ident: $field_type: ty
    ) => {
        // ignore, not handled by us
    };
}

#[allow(unused_imports)]
pub use cmdline_struct;
