pub struct CapabilityGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub capabilities: Vec<Capability>,
}

pub struct Capability {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}
