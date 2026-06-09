import 'package:besfa_editor/features/runtime_preview/domain/runtime_preview_status.dart';
import 'package:flutter/material.dart';

/// Toolbar for project actions and resident scene runtime controls.
class EditorTopBar extends StatelessWidget {
  const EditorTopBar({
    required this.runtimeStatus,
    required this.playbackState,
    required this.runtimeMessage,
    required this.isRuntimeBusy,
    required this.onPlayScene,
    required this.onStopScene,
    required this.onCreateCube,
    required this.onReloadRuntime,
    required this.onRestartRuntime,
    super.key,
  });

  final RuntimePreviewStatus runtimeStatus;
  final RuntimeScenePlaybackState playbackState;
  final String? runtimeMessage;
  final bool isRuntimeBusy;

  /// Starts game-time playback in the active runtime scene.
  final VoidCallback onPlayScene;

  /// Stops game-time playback and resets the active runtime scene.
  final VoidCallback onStopScene;

  /// Creates a cube in the active runtime scene.
  final VoidCallback onCreateCube;

  /// Reloads the currently running scene, starting the runtime if needed.
  final VoidCallback onReloadRuntime;

  /// Restarts the editor-owned scene runtime process.
  final VoidCallback onRestartRuntime;

  @override
  Widget build(BuildContext context) {
    final colorScheme = Theme.of(context).colorScheme;
    final statusColor = runtimeStatus.color(colorScheme);
    final isRuntimeReady =
        runtimeStatus == RuntimePreviewStatus.running && !isRuntimeBusy;

    return Container(
      height: 52,
      padding: const EdgeInsets.symmetric(horizontal: 16),
      decoration: BoxDecoration(
        color: colorScheme.surface,
        border: Border(
          bottom: BorderSide(color: Theme.of(context).dividerColor),
        ),
      ),
      child: Row(
        children: [
          const Icon(Icons.two_wheeler, size: 22),
          const SizedBox(width: 10),
          Text('Besfa', style: Theme.of(context).textTheme.titleMedium),
          const Spacer(),
          Icon(Icons.circle, size: 10, color: statusColor),
          const SizedBox(width: 8),
          Text(
            runtimeStatus.label,
            style: Theme.of(context).textTheme.labelMedium?.copyWith(
              color: statusColor,
              fontWeight: FontWeight.w600,
            ),
          ),
          if (runtimeStatus == RuntimePreviewStatus.running) ...[
            const SizedBox(width: 8),
            Text(
              playbackState.label,
              style: Theme.of(context).textTheme.labelMedium?.copyWith(
                color: playbackState.color(colorScheme),
                fontWeight: FontWeight.w600,
              ),
            ),
          ],
          if (runtimeMessage case final message?) ...[
            const SizedBox(width: 8),
            ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 320),
              child: Text(
                message,
                overflow: TextOverflow.ellipsis,
                maxLines: 1,
                style: Theme.of(context).textTheme.bodySmall?.copyWith(
                  color: colorScheme.onSurfaceVariant,
                ),
              ),
            ),
          ],
          const SizedBox(width: 12),
          IconButton(
            tooltip: 'Open project',
            onPressed: () {},
            icon: const Icon(Icons.folder_open),
          ),
          IconButton(
            tooltip: 'Play scene',
            onPressed:
                isRuntimeReady &&
                    playbackState == RuntimeScenePlaybackState.stopped
                ? onPlayScene
                : null,
            icon: const Icon(Icons.play_arrow),
          ),
          IconButton(
            tooltip: 'Stop scene',
            onPressed:
                isRuntimeReady &&
                    playbackState == RuntimeScenePlaybackState.playing
                ? onStopScene
                : null,
            icon: const Icon(Icons.stop),
          ),
          IconButton(
            tooltip: 'Add cube',
            onPressed: isRuntimeReady ? onCreateCube : null,
            icon: const Icon(Icons.add_box),
          ),
          IconButton(
            tooltip: 'Reload scene',
            onPressed: isRuntimeBusy ? null : onReloadRuntime,
            icon: const Icon(Icons.refresh),
          ),
          IconButton(
            tooltip: 'Restart scene runtime',
            onPressed: isRuntimeBusy ? null : onRestartRuntime,
            icon: const Icon(Icons.restart_alt),
          ),
        ],
      ),
    );
  }
}

extension on RuntimePreviewStatus {
  Color color(ColorScheme colorScheme) {
    return switch (this) {
      RuntimePreviewStatus.stopped => colorScheme.outline,
      RuntimePreviewStatus.starting => colorScheme.tertiary,
      RuntimePreviewStatus.running => colorScheme.primary,
      RuntimePreviewStatus.failed => colorScheme.error,
    };
  }
}

extension on RuntimeScenePlaybackState {
  Color color(ColorScheme colorScheme) {
    return switch (this) {
      RuntimeScenePlaybackState.stopped => colorScheme.outline,
      RuntimeScenePlaybackState.playing => colorScheme.secondary,
    };
  }
}
