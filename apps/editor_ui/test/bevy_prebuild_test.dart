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

  test('reads the share of finished units from cargo\'s progress bar', () async {
    final prebuild = BevyPrebuild();
    final statuses = <(String, double?)>[];
    prebuild.addListener(
      () => statuses.add((prebuild.value.message, prebuild.value.progress)),
    );

    // Cargo ends each progress redraw with a bare carriage return.
    await prebuild.start(
      executable: 'powershell',
      arguments: [
        '-NoProfile',
        '-Command',
        r'[Console]::Error.Write("   Compiling bevy v0.19.1`n    Building [==   ] 1/4: bevy`r    Building [====  ] 3/4: bevy`r   Compiling game v0.1.0`n")',
      ],
    );

    expect(statuses, [
      ('Compiling bevy v0.19.1', null),
      ('Compiling bevy v0.19.1', 0.25),
      ('Compiling bevy v0.19.1', 0.75),
      ('Compiling game v0.1.0', 0.75),
      ('Bevy is ready', null),
    ]);
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
