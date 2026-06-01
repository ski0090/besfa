import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:besfa_editor/widgets/editor_shell/editor_viewport.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
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
              onEditorCameraInput: (_) {},
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
              onEditorCameraInput: (_) {},
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

  testWidgets('sends editor camera rotation from secondary mouse drag', (
    tester,
  ) async {
    double? pickedX;
    final inputs = <EditorCameraInput>[];
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
              onPickViewport: (viewportX, _) {
                pickedX = viewportX;
              },
              onEditorCameraInput: inputs.add,
            ),
          ),
        ),
      ),
    );

    final gesture = await tester.createGesture(
      kind: PointerDeviceKind.mouse,
      buttons: kSecondaryMouseButton,
    );
    await gesture.down(tester.getCenter(find.byType(Texture)));
    await tester.pump();
    await gesture.moveBy(const Offset(14, -7));
    await tester.pump();
    await gesture.up();

    expect(pickedX, isNull);
    expect(inputs, isNotEmpty);
    expect(inputs.last.rotateDeltaX, 14);
    expect(inputs.last.rotateDeltaY, -7);
  });

  testWidgets('sends accelerated keyboard movement while rotating camera', (
    tester,
  ) async {
    final inputs = <EditorCameraInput>[];
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
              onEditorCameraInput: inputs.add,
            ),
          ),
        ),
      ),
    );

    final gesture = await tester.createGesture(
      kind: PointerDeviceKind.mouse,
      buttons: kSecondaryMouseButton,
    );
    await gesture.down(tester.getCenter(find.byType(Texture)));
    await tester.pump();
    await tester.sendKeyDownEvent(LogicalKeyboardKey.keyW);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pump(const Duration(milliseconds: 20));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.keyW);
    await gesture.up();

    final movementInput = inputs.lastWhere((input) => input.moveForward > 0);
    expect(movementInput.moveForward, 1);
    expect(movementInput.speedMultiplier, 4);
    expect(movementInput.deltaSeconds, greaterThan(0));
  });
}
