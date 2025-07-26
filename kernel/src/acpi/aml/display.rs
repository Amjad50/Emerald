use core::{
    cell::RefCell,
    fmt::{self, Write},
};

#[derive(Default)]
struct PadAdapterState {
    on_newline: bool,
}

struct PadAdapter<'buf, 'state> {
    buf: &'buf mut (dyn fmt::Write + 'buf),
    state: &'state mut PadAdapterState,
}

impl<'buf, 'state> PadAdapter<'buf, 'state> {
    pub fn wrap(buf: &'buf mut dyn fmt::Write, state: &'state mut PadAdapterState) -> Self {
        Self { buf, state }
    }
}

impl fmt::Write for PadAdapter<'_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for s in s.split_inclusive('\n') {
            if self.state.on_newline {
                self.buf.write_str("    ")?;
            }

            self.state.on_newline = s.ends_with('\n');
            self.buf.write_str(s)?;
        }

        Ok(())
    }

    fn write_char(&mut self, c: char) -> fmt::Result {
        if self.state.on_newline {
            self.buf.write_str("    ")?;
        }
        self.state.on_newline = c == '\n';
        self.buf.write_char(c)
    }
}

struct FmtHolder<F>
where
    F: FnOnce(&mut fmt::Formatter<'_>) -> fmt::Result,
{
    f: RefCell<Option<F>>,
}

impl<F> FmtHolder<F>
where
    F: FnOnce(&mut fmt::Formatter<'_>) -> fmt::Result,
{
    fn new(value_fmt: F) -> Self {
        Self {
            f: RefCell::new(Some(value_fmt)),
        }
    }
}

impl<F> fmt::Display for FmtHolder<F>
where
    F: FnOnce(&mut fmt::Formatter<'_>) -> fmt::Result,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (self.f.borrow_mut().take().unwrap())(f)
    }
}

pub struct AmlDisplayer<'a, 'b: 'a> {
    fmt: &'a mut fmt::Formatter<'b>,
    result: fmt::Result,
    in_paren_arg: bool,
    already_in_paren_arg: bool,
    already_in_body: bool,
    in_body: bool,
    is_list: bool,
}

impl<'a, 'b: 'a> AmlDisplayer<'a, 'b> {
    pub fn start(fmt: &'a mut fmt::Formatter<'b>, name: &str) -> Self {
        let result = fmt.write_str(name);
        Self {
            fmt,
            result,
            in_paren_arg: false,
            already_in_paren_arg: false,
            in_body: false,
            is_list: false,
            already_in_body: false,
        }
    }

    pub fn set_list(&mut self, value: bool) -> &mut Self {
        self.is_list = value;
        self
    }

    pub fn paren_arg<F>(&mut self, value_fmt: F) -> &mut Self
    where
        F: FnOnce(&mut fmt::Formatter<'_>) -> fmt::Result,
    {
        if !self.in_paren_arg && self.already_in_paren_arg {
            self.result = Err(fmt::Error);
        }

        self.result = self.result.and_then(|_| {
            let prefix = if self.in_paren_arg { ", " } else { " (" };

            self.fmt.write_str(prefix)?;
            value_fmt(self.fmt)
        });

        self.in_paren_arg = true;
        self.already_in_paren_arg = true;

        self
    }

    pub fn finish_paren_arg(&mut self) -> &mut Self {
        if self.in_paren_arg {
            self.result = self.result.and_then(|_| self.fmt.write_str(")"));
            self.in_paren_arg = false;
        }
        self
    }

    pub fn body_field<F>(&mut self, value_fmt: F) -> &mut Self
    where
        F: FnOnce(&mut fmt::Formatter<'_>) -> fmt::Result,
    {
        self.finish_paren_arg();

        self.result = self.result.and_then(|_| {
            if self.fmt.alternate() {
                if !self.in_body {
                    self.fmt.write_str(" {\n")?;
                } else {
                    self.fmt
                        .write_str(if self.is_list { ",\n" } else { "\n" })?;
                }

                let mut state = PadAdapterState { on_newline: true };
                let mut writer = PadAdapter::wrap(self.fmt, &mut state);
                let fmt_holder = FmtHolder::new(value_fmt);

                writer.write_fmt(format_args!("{fmt_holder:#}"))
            } else {
                let prefix = if self.in_body {
                    if self.is_list {
                        ", "
                    } else {
                        "; "
                    }
                } else {
                    " { "
                };

                self.fmt.write_str(prefix)?;
                value_fmt(self.fmt)
            }
        });

        self.in_body = true;
        self.already_in_body = true;

        self
    }

    pub fn at_least_empty_paren_arg(&mut self) -> &mut Self {
        if !self.in_paren_arg && !self.already_in_paren_arg {
            self.result = self.result.and_then(|_| self.fmt.write_str(" ()"));
        }
        self
    }

    pub fn at_least_empty_body(&mut self) -> &mut Self {
        if !self.in_body && !self.already_in_body {
            self.result = self.result.and_then(|_| self.fmt.write_str("{ }"));
        }
        self
    }

    pub fn finish(&mut self) -> fmt::Result {
        self.finish_paren_arg();

        if self.in_body {
            self.result = self.result.and_then(|_| {
                if !self.fmt.alternate() {
                    self.fmt.write_str(" }")
                } else {
                    self.fmt.write_str("\n}")
                }
            });
            self.in_body = false;
        }

        self.result
    }
}

pub struct HexHolder<'a, T: fmt::UpperHex>(pub &'a T);

impl<T: fmt::UpperHex> fmt::Display for HexHolder<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#X}", self.0)
    }
}
