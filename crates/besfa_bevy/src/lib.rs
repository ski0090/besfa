use besfa_core::EngineInfo;

pub fn integration_name() -> String {
    let info = EngineInfo::current();
    format!("{} Bevy integration", info.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_integration_name() {
        assert_eq!(integration_name(), "Besfa Bevy integration");
    }
}
