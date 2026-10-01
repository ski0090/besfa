import 'dart:io';

import 'package:flutter/foundation.dart';

import 'package:editor_ui/shared/process/cli_process.dart';

enum BevyPrebuildPhase { running, ready, failed }

/// What the background `besfa prebuild` is doing, as one line for the UI.
class BevyPrebuildStatus {
  const BevyPrebuildStatus(this.phase, this.message, {this.progress});

  final BevyPrebuildPhase phase;
  final String message;

  /// Share of cargo's build units finished, 0 to 1; null until cargo
  /// reports it, and when nothing needs building.
  final double? progress;
}

/// Cargo's progress bar, e.g. `Building [==>   ] 210/498: bevy_render…`.
final _buildProgress = RegExp(r'^Building \[[^\]]*\]\s*(\d+)/(\d+)');

/// Runs `besfa prebuild` once and publishes its progress: cargo's latest
/// output line and share of finished units while it runs, then whether Bevy
/// is ready.
class BevyPrebuild extends ValueNotifier<BevyPrebuildStatus> {
  BevyPrebuild()
    : super(
        const BevyPrebuildStatus(BevyPrebuildPhase.running, 'Preparing Bevy…'),
      );

  Future<void> start({
    String executable = 'besfa',
    List<String> arguments = const ['prebuild'],
  }) async {
    try {
      final process = await CliProcess.start(
        '.',
        executable: executable,
        arguments: arguments,
        // Cargo draws its progress bar only on a terminal unless forced, and
        // a forced bar needs a width. On a pipe each redraw ends with '\r',
        // which LineSplitter splits on.
        environment: const {
          'CARGO_TERM_PROGRESS_WHEN': 'always',
          'CARGO_TERM_PROGRESS_WIDTH': '80',
        },
        onOutput: (line) {
          line = line.trim();
          if (line.isEmpty) {
            return;
          }
          final progress = _buildProgress.firstMatch(line);
          value = progress == null
              ? BevyPrebuildStatus(
                  BevyPrebuildPhase.running,
                  line,
                  progress: value.progress,
                )
              : BevyPrebuildStatus(
                  BevyPrebuildPhase.running,
                  value.message,
                  progress: int.parse(progress[1]!) / int.parse(progress[2]!),
                );
        },
      );
      final code = await process.exitCode;
      value = code == 0
          ? const BevyPrebuildStatus(BevyPrebuildPhase.ready, 'Bevy is ready')
          : BevyPrebuildStatus(
              BevyPrebuildPhase.failed,
              'Bevy prebuild failed (exit $code)',
            );
    } on ProcessException catch (error) {
      // Run still works without it; it just builds Bevy itself.
      value = BevyPrebuildStatus(
        BevyPrebuildPhase.failed,
        'Could not start $executable: ${error.message.trim()}',
      );
    }
  }
}
