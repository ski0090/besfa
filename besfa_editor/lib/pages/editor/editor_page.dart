import 'dart:async';

import 'package:besfa_editor/features/runtime_preview/application/runtime_preview_controller.dart';
import 'package:besfa_editor/widgets/editor_shell/editor_top_bar.dart';
import 'package:besfa_editor/widgets/editor_shell/editor_viewport.dart';
import 'package:besfa_editor/widgets/editor_shell/inspector_panel.dart';
import 'package:besfa_editor/widgets/editor_shell/runtime_log_panel.dart';
import 'package:besfa_editor/widgets/editor_shell/scene_tree_panel.dart';
import 'package:flutter/material.dart';

/// Main desktop editor shell with an always-on scene runtime.
class EditorPage extends StatefulWidget {
  const EditorPage({
    @visibleForTesting this.runtimePreviewController,
    super.key,
  });

  /// Controller override used by widget tests.
  @visibleForTesting
  final RuntimePreviewController? runtimePreviewController;

  @override
  State<EditorPage> createState() => _EditorPageState();
}

class _EditorPageState extends State<EditorPage> {
  late final RuntimePreviewController _runtimePreviewController;
  late final bool _ownsRuntimePreviewController;

  @override
  void initState() {
    super.initState();
    _runtimePreviewController =
        widget.runtimePreviewController ?? RuntimePreviewController();
    _ownsRuntimePreviewController = widget.runtimePreviewController == null;
    unawaited(_runtimePreviewController.ensureRuntimeReady());
  }

  @override
  void dispose() {
    if (_ownsRuntimePreviewController) {
      _runtimePreviewController.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _runtimePreviewController,
      builder: (context, _) {
        final runtimePreview = _runtimePreviewController;

        return Scaffold(
          body: SafeArea(
            child: Column(
              children: [
                EditorTopBar(
                  runtimeStatus: runtimePreview.status,
                  runtimeMessage: runtimePreview.message,
                  isRuntimeBusy: runtimePreview.isBusy,
                  onCreateCube: runtimePreview.createCube,
                  onReloadRuntime: runtimePreview.reloadRuntime,
                  onRestartRuntime: runtimePreview.restartRuntime,
                ),
                Expanded(
                  child: Row(
                    children: [
                      SizedBox(
                        width: 240,
                        child: SceneTreePanel(
                          snapshot: runtimePreview.sceneSnapshot,
                          onSelectEntity: runtimePreview.selectEntity,
                        ),
                      ),
                      const VerticalDivider(width: 1),
                      Expanded(
                        child: EditorViewport(
                          platformVersion: runtimePreview.platformVersion,
                          abiVersion: runtimePreview.abiVersion,
                          runtimeStatus: runtimePreview.status,
                          runtimeMessage: runtimePreview.message,
                          frameStats: runtimePreview.frameStats,
                          editorCameraState: runtimePreview.editorCameraState,
                          previewTextureId: runtimePreview.previewTextureId,
                          onPickViewport: (viewportX, viewportY) {
                            unawaited(
                              runtimePreview.pickViewportEntity(
                                viewportX: viewportX,
                                viewportY: viewportY,
                              ),
                            );
                          },
                          onEditorCameraInput: (input) {
                            unawaited(
                              runtimePreview.applyEditorCameraInput(
                                rotateDeltaX: input.rotateDeltaX,
                                rotateDeltaY: input.rotateDeltaY,
                                moveForward: input.moveForward,
                                moveRight: input.moveRight,
                                moveUp: input.moveUp,
                                speedMultiplier: input.speedMultiplier,
                                deltaSeconds: input.deltaSeconds,
                              ),
                            );
                          },
                        ),
                      ),
                      const VerticalDivider(width: 1),
                      SizedBox(
                        width: 280,
                        child: InspectorPanel(
                          selectedEntity:
                              runtimePreview.sceneSnapshot?.selectedEntity,
                          onSetTranslation:
                              runtimePreview.setSelectedEntityTranslation,
                        ),
                      ),
                    ],
                  ),
                ),
                RuntimeLogPanel(logs: runtimePreview.logs),
              ],
            ),
          ),
        );
      },
    );
  }
}
