import 'package:besfa_editor/features/runtime_ipc/domain/runtime_ipc_models.dart';
import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:flutter/material.dart';

class EditorViewport extends StatelessWidget {
  const EditorViewport({
    required this.platformVersion,
    required this.abiVersion,
    required this.runtimeStatus,
    required this.runtimeMessage,
    required this.frameStats,
    required this.previewTextureId,
    super.key,
  });

  final Future<String?> platformVersion;
  final int abiVersion;
  final RuntimePreviewStatus runtimeStatus;
  final String? runtimeMessage;
  final RuntimeFrameStats? frameStats;

  /// Flutter texture id for the native preview surface, when available.
  final int? previewTextureId;

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: const Color(0xFFF4F1EA),
      child: Stack(
        children: [
          Center(
            child: Container(
              width: 520,
              height: 320,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: const Color(0xFF202124),
                borderRadius: BorderRadius.circular(6),
              ),
              clipBehavior: Clip.antiAlias,
              child: previewTextureId == null
                  ? const Text(
                      'Preview surface',
                      style: TextStyle(color: Colors.white70),
                    )
                  : Texture(textureId: previewTextureId!),
            ),
          ),
          Positioned(
            left: 16,
            right: 16,
            bottom: 16,
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
                  style: Theme.of(context).textTheme.bodySmall,
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}
