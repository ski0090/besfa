import 'package:besfa_editor/features/runtime_preview/application/runtime_preview_controller.dart';
import 'package:besfa_editor/widgets/editor_shell/editor_top_bar.dart';
import 'package:besfa_editor/widgets/editor_shell/editor_viewport.dart';
import 'package:besfa_editor/widgets/editor_shell/inspector_panel.dart';
import 'package:besfa_editor/widgets/editor_shell/scene_tree_panel.dart';
import 'package:flutter/material.dart';

class EditorPage extends StatefulWidget {
  const EditorPage({super.key});

  @override
  State<EditorPage> createState() => _EditorPageState();
}

class _EditorPageState extends State<EditorPage> {
  late final RuntimePreviewController _runtimePreviewController;

  @override
  void initState() {
    super.initState();
    _runtimePreviewController = RuntimePreviewController();
  }

  @override
  void dispose() {
    _runtimePreviewController.dispose();
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
                  onRunPreview: runtimePreview.runPreview,
                  onStopPreview: runtimePreview.stopPreview,
                  onReloadRuntime: runtimePreview.reloadRuntime,
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
                          previewTextureId: runtimePreview.previewTextureId,
                        ),
                      ),
                      const VerticalDivider(width: 1),
                      const SizedBox(width: 280, child: InspectorPanel()),
                    ],
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}
