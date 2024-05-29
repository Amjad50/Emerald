pub struct Tokenizer<'a> {
    running_str: &'a str,
    idx: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(inp: &'a str) -> Self {
        Self {
            running_str: inp,
            idx: 0,
        }
    }

    pub fn current_index(&self) -> usize {
        self.idx
    }

    fn next_token<P1, P2, P3>(
        &mut self,
        find_pattern: P1,
        starts_with_pattern: P2,
        strip_pattern: P3,
    ) -> Option<(usize, &'a str)>
    where
        P1: FnMut(char) -> bool,
        P2: FnMut(char) -> bool,
        P3: FnMut(char) -> bool,
    {
        if self.running_str.is_empty() {
            return None;
        }

        let i = self.running_str.find(find_pattern)?;

        let (value, rest) = self.running_str.split_at(i);

        if value.is_empty() {
            return None;
        }

        let rest_len = rest.len();

        self.running_str = rest
            .strip_prefix(starts_with_pattern)?
            .trim_start_matches(strip_pattern);

        let old_value_len = value.len();
        let value = value.trim_start();

        let leading_whitespace_size = value.len() - old_value_len;
        let pos_start = self.idx + leading_whitespace_size;
        self.idx += i + (rest_len - self.running_str.len());

        Some((pos_start, value.trim_end()))
    }

    pub fn next_ident(&mut self) -> Option<(usize, &'a str)> {
        self.next_token(
            |c| !c.is_alphanumeric() && c != ',' && c != '_',
            |c| c == '=',
            |c| c.is_whitespace(),
        )
    }

    pub fn next_value(&mut self) -> Option<(usize, &'a str)> {
        self.next_token(
            |c| c.is_whitespace() || c == ',' || c == '=',
            |c| c.is_whitespace() || c == ',',
            |c| c.is_whitespace() || c == ',',
        )
        .or_else(|| {
            let rest = self.running_str;
            self.running_str = "";
            let pos_start = self.idx;
            self.idx += rest.len();
            Some((pos_start, rest))
        })
    }
}
