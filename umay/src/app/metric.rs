pub struct Metrics {
    pub name: String,
    pub value: f64,
}

impl Metrics {
    pub fn new(name: String, value: f64) -> Self {
        Self { name, value }
    }
}
