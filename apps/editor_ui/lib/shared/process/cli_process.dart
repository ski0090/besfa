import 'dart:convert';
import 'dart:io';

/// A command-line process started by the editor, such as a game's
/// `cargo run`, with its output reported line by line.
class CliProcess {
  CliProcess._(this._process, this.exitCode);

  final Process _process;

  /// Completes once the process has exited and all of its output has been
  /// reported, so no line arrives after the exit code.
  final Future<int> exitCode;

  /// Starts [executable] in [directory] and reports stdout and stderr lines
  /// to [onOutput]. Cargo writes its build progress to stderr.
  static Future<CliProcess> start(
    String directory, {
    required String executable,
    required List<String> arguments,
    required void Function(String line) onOutput,
    Map<String, String>? environment,
  }) async {
    final process = await Process.start(
      executable,
      arguments,
      workingDirectory: directory,
      // The log panel shows plain text, not ANSI colors.
      environment: {'NO_COLOR': '1', ...?environment},
    );
    final drained = [
      for (final stream in [process.stdout, process.stderr])
        stream
            .transform(const Utf8Decoder(allowMalformed: true))
            .transform(const LineSplitter())
            .listen(onOutput)
            .asFuture<void>(),
    ];
    // ponytail: a child that inherits the pipes (a game left behind when
    // cargo is killed from outside without /T) keeps this pending; fall back
    // to process.exitCode after a grace period if that ever shows up.
    return CliProcess._(
      process,
      Future.wait(drained).then((_) => process.exitCode),
    );
  }

  /// Stops the process and everything it launched.
  Future<void> stop() async {
    if (Platform.isWindows) {
      // Killing only cargo would leave the game it launched as an orphan.
      await Process.run('taskkill', ['/PID', '${_process.pid}', '/T', '/F']);
    } else {
      _process.kill();
    }
  }
}
