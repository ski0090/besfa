fn main() {
    besfa_bevy::run_preview_runtime_with_options(parse_options(std::env::args().skip(1)));
}

fn parse_options(args: impl IntoIterator<Item = String>) -> besfa_bevy::PreviewRuntimeOptions {
    let mut options = besfa_bevy::PreviewRuntimeOptions::default();
    let mut ipc_port = None;
    let mut ipc_token = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--ipc-port=") {
            ipc_port = value.parse::<u16>().ok();
        } else if arg == "--ipc-port" {
            ipc_port = args.next().and_then(|value| value.parse::<u16>().ok());
        } else if let Some(value) = arg.strip_prefix("--ipc-token=") {
            ipc_token = value.parse::<u64>().ok();
        } else if arg == "--ipc-token" {
            ipc_token = args.next().and_then(|value| value.parse::<u64>().ok());
        } else if let Some(value) = arg.strip_prefix("--scene=") {
            options.scene_path = Some(value.into());
        } else if arg == "--scene" {
            options.scene_path = args.next().map(Into::into);
        }
    }

    if let (Some(port), Some(token)) = (ipc_port, ipc_token) {
        options.ipc = Some(besfa_bevy::RuntimeIpcConfig::new(port, token));
    }

    options
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_ipc_options() {
        let options = super::parse_options([
            "--ipc-port".to_string(),
            "49152".to_string(),
            "--ipc-token=42".to_string(),
        ]);

        let ipc = options.ipc.unwrap();
        assert_eq!(ipc.port, 49152);
        assert_eq!(ipc.token, 42);
    }

    #[test]
    fn parses_scene_option() {
        let options = super::parse_options(["--scene=C:/project/Scene.besfa.json".to_string()]);

        assert_eq!(
            options.scene_path.unwrap(),
            std::path::PathBuf::from("C:/project/Scene.besfa.json")
        );
    }
}
