import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/widgets/editor_shell/inspector_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('shows selected camera preview below position', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: InspectorPanel(
            selectedEntity: const RuntimeSceneEntity(
              id: 'camera_3d',
              name: 'Camera3d',
              kind: 'camera',
              transform: RuntimeSceneTransform(
                translation: RuntimeVector3(x: 1, y: 2, z: 3),
              ),
              children: [],
            ),
            cameraPreviewTextureId: 22,
            onSetTranslation: (_) {},
          ),
        ),
      ),
    );

    expect(find.text('Position'), findsOneWidget);
    expect(find.text('Camera Preview'), findsOneWidget);
    expect(find.byType(Texture), findsOneWidget);
  });
}
