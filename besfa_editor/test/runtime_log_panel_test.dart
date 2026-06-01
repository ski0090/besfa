import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/widgets/editor_shell/runtime_log_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('expands and copies runtime logs', (tester) async {
    String? copiedText;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(SystemChannels.platform, (call) async {
          if (call.method == 'Clipboard.setData') {
            final arguments = call.arguments;
            if (arguments is Map) {
              copiedText = arguments['text'] as String?;
            }
          }
          return null;
        });
    addTearDown(() {
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(SystemChannels.platform, null);
    });

    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: RuntimeLogPanel(
            logs: [
              RuntimeLogEntry(level: 'info', message: 'Runtime ready'),
              RuntimeLogEntry(level: 'warn', message: 'Slow frame'),
            ],
          ),
        ),
      ),
    );

    expect(find.text('[WARN] Slow frame'), findsOneWidget);

    await tester.tap(find.byTooltip('Expand logs'));
    await tester.pumpAndSettle();

    expect(find.text('Runtime ready'), findsOneWidget);
    expect(find.text('Slow frame'), findsWidgets);

    await tester.tap(find.byTooltip('Copy logs'));
    await tester.pump();

    expect(copiedText, '[INFO] Runtime ready\n[WARN] Slow frame');
  });
}
