import 'package:flutter_test/flutter_test.dart';

import 'package:editor_ui/shared/process/cli_process.dart';

void main() {
  test('reports every output line before the exit code', () async {
    final lines = <String>[];
    final process = await CliProcess.start(
      '.',
      executable: 'cmd',
      arguments: ['/c', 'echo hello'],
      onOutput: lines.add,
    );

    expect(await process.exitCode, 0);
    expect(lines, ['hello']);
  });

  test('stop ends the whole process tree', () async {
    final process = await CliProcess.start(
      '.',
      executable: 'cmd',
      arguments: ['/c', 'ping -n 30 127.0.0.1 > nul'],
      onOutput: (_) {},
    );

    await process.stop();
    expect(
      await process.exitCode.timeout(const Duration(seconds: 5)),
      isNot(0),
    );
  });
}
