import 'dart:io';

import 'package:flutter/foundation.dart';

import 'package:editor_ui/features/run_game/model/game_process.dart';

/// Builds Bevy in the background while the hub is open, so the first Run of
/// a game only compiles the game. A Run started earlier waits on cargo's
/// build-directory lock, then reuses what the prebuild finished.
Future<void> startBevyPrebuild() async {
  try {
    final prebuild = await GameProcess.start(
      '.',
      onOutput: debugPrint,
      executable: 'besfa',
      arguments: const ['prebuild'],
    );
    final code = await prebuild.exitCode;
    if (code != 0) debugPrint('The Bevy prebuild exited with code $code.');
  } on ProcessException catch (error) {
    // Run still works without it; it just builds Bevy itself.
    debugPrint('Could not start the Bevy prebuild: ${error.message}');
  }
}
