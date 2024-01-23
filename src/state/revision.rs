#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Revision(Vec<usize>);

impl Default for Revision {
    fn default() -> Self {
        Self(vec![0])
    }
}

impl From<Vec<usize>> for Revision {
    fn from(value: Vec<usize>) -> Self {
        Revision(value)
    }
}

impl std::fmt::Display for Revision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self
            .0
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<String>>()
            .join("-");
        f.write_str(&s)
    }
}

impl Revision {
    pub fn increment(mut self) -> Self {
        assert_eq!(self.0.len(), 1);
        self.0[0] += 1;
        self
    }

    pub fn components(&self) -> &[usize] {
        &self.0
    }
}
