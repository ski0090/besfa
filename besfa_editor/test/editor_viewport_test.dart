import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:besfa_editor/widgets/editor_shell/editor_viewport.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('normalizes preview surface tap coordinates', (tester) async {
    double? pickedX;
    double? pickedY;

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 800,
            height: 600,
            child: EditorViewport(
              platformVersion: Future.value('test platform'),
              abiVersion: 1,
              runtimeStatus: RuntimePreviewStatus.running,
              runtimeMessage: null,
              frameStats: null,
              previewTextureId: 1,
              onPickViewport: (viewportX, viewportY) {
                pickedX = viewportX;
                pickedY = viewportY;
              },
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.byType(Texture));
    await tester.pump();

    expect(pickedX, closeTo(0.5, 0.01));
    expect(pickedY, closeTo(0.5, 0.01));
  });

  testWidgets('pins the axis gizmo to the preview surface corner', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 800,
            height: 600,
            child: EditorViewport(
              platformVersion: Future.value('test platform'),
              abiVersion: 1,
              runtimeStatus: RuntimePreviewStatus.running,
              runtimeMessage: null,
              frameStats: null,
              previewTextureId: 1,
              onPickViewport: (_, _) {},
            ),
          ),
        ),
      ),
    );

    final textureRect = tester.getRect(find.byType(Texture));
    final gizmoRect = tester.getRect(
      find.byKey(const ValueKey('viewportAxisGizmo')),
    );

    expect(gizmoRect.left, closeTo(textureRect.left + 12, 0.01));
    expect(gizmoRect.top, closeTo(textureRect.top + 12, 0.01));
  });
}
