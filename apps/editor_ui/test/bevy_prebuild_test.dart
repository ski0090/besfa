import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/features/prebuild_bevy/model/bevy_prebuild.dart';

void main() {
  test('shows the latest output line, then the result', () async {
    final prebuild = BevyPrebuild();
    final messages = <String>[];
    prebuild.addListener(() => messages.add(prebuild.value.message));

    await prebuild.start(
      executable: 'cmd',
      arguments: ['/c', 'echo Compiling bevy && exit 3'],
    );

    expect(messages, ['Compiling bevy', 'Bevy prebuild failed (exit 3)']);
    expect(prebuild.value.phase, BevyPrebuildPhase.failed);
  });

  test('reports a CLI that cannot start', () async {
    final prebuild = BevyPrebuild();

    await prebuild.start(executable: 'besfa-missing-cli');

    expect(prebuild.value.phase, BevyPrebuildPhase.failed);
    expect(
      prebuild.value.message,
      startsWith('Could not start besfa-missing-cli'),
    );
  });
}
