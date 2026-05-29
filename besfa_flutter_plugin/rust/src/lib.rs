#[unsafe(no_mangle)]
pub extern "C" fn besfa_flutter_plugin_abi_version() -> u32 {
    besfa_core::ABI_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn besfa_flutter_plugin_add(left: i32, right: i32) -> i32 {
    left + right
}

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_abi_version() {
        assert_eq!(
            crate::besfa_flutter_plugin_abi_version(),
            besfa_core::ABI_VERSION
        );
    }
}
