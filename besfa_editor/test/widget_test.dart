// This is a basic Flutter widget test.
//
// To perform an interaction with a widget in your test, use the WidgetTester
// utility in the flutter_test package. For example, you can send tap and scroll
// gestures. You can also use WidgetTester to find child widgets in the widget
// tree, read text, and verify that the values of widget properties are correct.

import 'package:besfa_editor/features/runtime_preview/application/runtime_preview_controller.dart';
import 'package:besfa_editor/pages/editor/editor_page.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'helpers/runtime_preview_fakes.dart';

void main() {
  testWidgets('shows editor shell', (WidgetTester tester) async {
    final plugin = FakeBesfaFlutterPlugin();
    final ipcClient = FakeRuntimeIpcClient();
    final controller = RuntimePreviewController(
      plugin: plugin,
      ipcClient: ipcClient,
    );

    try {
      await controller.ensureRuntimeReady();
      await tester.pumpWidget(
        MaterialApp(home: EditorPage(runtimePreviewController: controller)),
      );
      await tester.pump();

      expect(find.text('Besfa'), findsOneWidget);
      expect(find.text('Waiting for scene surface'), findsOneWidget);
    } finally {
      controller.dispose();
      await ipcClient.close();
    }
  });
}
