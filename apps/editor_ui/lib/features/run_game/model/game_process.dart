import 'dart:convert';
import 'dart:io';

/// A `cargo run` of a game project, started by the editor.
class GameProcess {
  GameProcess._(this._process);

  final Process _process;

  Future<int> get exitCode => _process.exitCode;

  /// Starts the game in [directory] and reports stdout and stderr lines to
  /// [onOutput]. Cargo writes its build progress to stderr.
  static Future<GameProcess> start(
    String directory, {
    required void Function(String line) onOutput,
    String executable = 'cargo',
    List<String> arguments = const ['run'],
  }) async {
    final process = await Process.start(
      executable,
      arguments,
      workingDirectory: directory,
    );
    for (final stream in [process.stdout, process.stderr]) {
      stream
          .transform(const Utf8Decoder(allowMalformed: true))
          .transform(const LineSplitter())
          .listen(onOutput);
    }
    return GameProcess._(process);
  }

  /// Stops cargo and the game executable it launched.
  Future<void> stop() async {
    if (Platform.isWindows) {
      // Killing only cargo would leave the game running as an orphan.
      await Process.run('taskkill', ['/PID', '${_process.pid}', '/T', '/F']);
    } else {
      _process.kill();
    }
  }
}
