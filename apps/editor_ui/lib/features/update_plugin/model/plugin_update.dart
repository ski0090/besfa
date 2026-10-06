import 'dart:io';

import 'package:editor_ui/shared/process/cli_process.dart';

/// A lock entry for the plugin as a git dependency:
///
/// ```toml
/// [[package]]
/// name = "besfa_editor_plugin"
/// version = "0.1.0"
/// source = "git+https://github.com/ski0090/besfa#<revision>"
/// ```
final _gitEntry = RegExp(
  r'name = "besfa_editor_plugin"\r?\n'
  r'version = "[^"]*"\r?\n'
  r'source = "git\+[^"#]*#([0-9a-f]+)"',
);

/// The git revision a Cargo.lock pins `besfa_editor_plugin` to, or null
/// when there is no lock yet or the plugin is a path dependency.
String? pluginRevision(String lockPath) {
  try {
    return _gitEntry.firstMatch(File(lockPath).readAsStringSync())?[1];
  } on FileSystemException {
    return null;
  }
}

/// The revision the shared Bevy prebuild was built with. `besfa prebuild`
/// updates it when the editor starts, so it is the latest the editor knows.
String? prebuildPluginRevision() {
  final local = Platform.environment['LOCALAPPDATA'];
  if (local == null) {
    return null;
  }
  return pluginRevision(
    [local, 'Besfa', 'prebuild', 'Cargo.lock'].join(Platform.pathSeparator),
  );
}

/// Moves the project's plugin to the latest commit; tests replace it.
typedef UpdatePlugin =
    Future<int> Function(
      String directory, {
      required void Function(String line) onOutput,
    });

/// Moves `besfa_editor_plugin` in the project's Cargo.lock to the latest
/// commit on GitHub, leaving every other package alone. Cargo reports what
/// changed, if anything, on [onOutput]; the exit code is returned.
Future<int> cargoUpdatePlugin(
  String directory, {
  required void Function(String line) onOutput,
}) async {
  final process = await CliProcess.start(
    directory,
    executable: 'cargo',
    arguments: const ['update', '-p', 'besfa_editor_plugin'],
    onOutput: onOutput,
  );
  return process.exitCode;
}
