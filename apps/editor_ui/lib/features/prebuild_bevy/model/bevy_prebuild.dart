import 'dart:io';

import 'package:flutter/foundation.dart';

import 'package:editor_ui/shared/process/cli_process.dart';

enum BevyPrebuildPhase { running, ready, failed }

/// What the background `besfa prebuild` is doing, as one line for the UI.
class BevyPrebuildStatus {
  const BevyPrebuildStatus(this.phase, this.message);

  final BevyPrebuildPhase phase;
  final String message;
}

/// Runs `besfa prebuild` once and publishes its progress: cargo's latest
/// output line while it runs, then whether Bevy is ready.
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
        onOutput: (line) {
          if (line.trim().isNotEmpty) {
            value = BevyPrebuildStatus(BevyPrebuildPhase.running, line.trim());
          }
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
