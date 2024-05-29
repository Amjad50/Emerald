#[macro_export]
macro_rules! cmdline_struct {
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

    // build the struct
    (
        @build_struct $vis:vis [$($name_ident: tt)*] { } => [$($attribs: tt)*] => [$($built: tt)*]
    ) => {
        $($attribs)*
        $vis struct $($name_ident)* {
            $($built)*
        }
    };

    // handle attributes
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

    // handle field
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

    // build the parser trait
    (
        @build_parser_attr
        #[default = $expr: expr]
        $field_name: ident: $field_type: ty
    ) => {
        $field_name = $expr;
    };
    (
        @build_parser_attr
        #[$($field_attrs: tt)*]
        $field_name: ident: $field_type: ty
    ) => {
        // ignore, not handled by us
    };
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
}

#[allow(unused_imports)]
pub use cmdline_struct;
