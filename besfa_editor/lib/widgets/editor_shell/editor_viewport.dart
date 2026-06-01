import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:flutter/material.dart';

/// Central Scene View surface backed by the editor-owned runtime.
class EditorViewport extends StatelessWidget {
  const EditorViewport({
    required this.platformVersion,
    required this.abiVersion,
    required this.runtimeStatus,
    required this.runtimeMessage,
    required this.frameStats,
    required this.previewTextureId,
    required this.onPickViewport,
    super.key,
  });

  final Future<String?> platformVersion;
  final int abiVersion;
  final RuntimePreviewStatus runtimeStatus;
  final String? runtimeMessage;
  final RuntimeFrameStats? frameStats;

  /// Flutter texture id for the native preview surface, when available.
  final int? previewTextureId;

  /// Called with normalized coordinates when the preview surface is clicked.
  final void Function(double viewportX, double viewportY) onPickViewport;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0xFF171A1A),
      child: Stack(
        children: [
          Positioned.fill(
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: Center(
                child: AspectRatio(
                  aspectRatio: 16 / 9,
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      return GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        onTapDown: previewTextureId == null
                            ? null
                            : (details) {
                                final size = constraints.biggest;
                                if (size.width <= 0 || size.height <= 0) {
                                  return;
                                }

                                onPickViewport(
                                  (details.localPosition.dx / size.width)
                                      .clamp(0, 1)
                                      .toDouble(),
                                  (details.localPosition.dy / size.height)
                                      .clamp(0, 1)
                                      .toDouble(),
                                );
                              },
                        child: DecoratedBox(
                          decoration: BoxDecoration(
                            color: const Color(0xFF101314),
                            border: Border.all(color: const Color(0xFF303637)),
                          ),
                          child: previewTextureId == null
                              ? Center(
                                  child: Text(
                                    _placeholderText(),
                                    style: Theme.of(context)
                                        .textTheme
                                        .bodyMedium
                                        ?.copyWith(color: Colors.white70),
                                  ),
                                )
                              : Texture(textureId: previewTextureId!),
                        ),
                      );
                    },
                  ),
                ),
              ),
            ),
          ),
          Positioned(
            left: 16,
            right: 16,
            bottom: 12,
            child: FutureBuilder<String?>(
              future: platformVersion,
              builder: (context, snapshot) {
                final platform = snapshot.data ?? 'platform pending';
                final runtimeText = runtimeMessage == null
                    ? runtimeStatus.label
                    : '${runtimeStatus.label}: $runtimeMessage';
                final statsText = frameStats == null
                    ? ''
                    : ' | ${frameStats!.fps.toStringAsFixed(0)} FPS';
                return Text(
                  '$platform | Rust ABI $abiVersion | $runtimeText$statsText',
                  overflow: TextOverflow.ellipsis,
                  maxLines: 1,
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: const Color(0xDBFFFFFF),
                  ),
                );
              },
            ),
          ),
        ],
      ),
    );
  }

  String _placeholderText() {
    return switch (runtimeStatus) {
      RuntimePreviewStatus.stopped => 'Scene runtime offline',
      RuntimePreviewStatus.starting => 'Starting scene runtime',
      RuntimePreviewStatus.running => 'Waiting for scene surface',
      RuntimePreviewStatus.failed => 'Scene runtime failed',
    };
  }
}
