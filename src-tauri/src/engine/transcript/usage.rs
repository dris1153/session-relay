use std::ops::{AddAssign, SubAssign};

use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_creation: u64,
}

impl AddAssign for Usage {
    fn add_assign(&mut self, o: Self) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_creation += o.cache_creation;
    }
}

impl SubAssign for Usage {
    fn sub_assign(&mut self, o: Self) {
        self.input -= o.input;
        self.output -= o.output;
        self.cache_read -= o.cache_read;
        self.cache_creation -= o.cache_creation;
    }
}
