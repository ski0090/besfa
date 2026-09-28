import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/features/run_game/model/game_process.dart';

void main() {
  test('reports output lines and the exit code', () async {
    final lines = <String>[];
    final game = await GameProcess.start(
      '.',
      onOutput: lines.add,
      executable: 'cmd',
      arguments: ['/c', 'echo hello'],
    );

    expect(await game.exitCode, 0);
    await pumpEventQueue();
    expect(lines, ['hello']);
  });

  test('stop ends the whole process tree', () async {
    final game = await GameProcess.start(
      '.',
      onOutput: (_) {},
      executable: 'cmd',
      arguments: ['/c', 'ping -n 30 127.0.0.1 > nul'],
    );

    await game.stop();
    expect(await game.exitCode.timeout(const Duration(seconds: 5)), isNot(0));
  });
}
