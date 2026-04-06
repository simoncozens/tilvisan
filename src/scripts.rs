use skrifa::outline::SCRIPT_CLASSES;

#[derive(Debug, Clone, Copy)]
pub struct ScriptClassIndex(usize);

impl ScriptClassIndex {
    pub fn from_tag(tag: &str) -> Result<ScriptClassIndex, String> {
        SCRIPT_CLASSES
            .iter()
            .position(|sc| sc.tag.to_string().to_lowercase() == tag)
            .map(ScriptClassIndex)
            .ok_or_else(|| format!("Invalid script tag: {}", tag))
    }

    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl std::fmt::Display for ScriptClassIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", SCRIPT_CLASSES[self.0].tag)
    }
}
